extern crate bytes;
extern crate meval;
extern crate prost;
extern crate rust_embed;
extern crate toml;

use self::meval::Expr;
use self::rust_embed::RustEmbed;
use super::frames::*;
use super::rotations::*;
use super::state::Orbit;
use super::xb::ephem_interp::StateData::{EqualStates, VarwindowStates};
use super::xb::{Ephemeris, Xb};
use super::SPEED_OF_LIGHT_KMS;
use crate::errors::NyxError;
use crate::hifitime::{Epoch, TimeUnit, SECONDS_PER_DAY};
use crate::io::frame_serde;
use crate::na::Matrix3;
use crate::utils::{capitalize, rotv};
use std::collections::HashMap;
use std::fmt;
pub use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::str::FromStr;
use std::sync::Arc;

#[derive(RustEmbed)]
#[folder = "data/embed/"]
struct EmbeddedAsset;

/// Mass of the solar system from https://en.wikipedia.org/w/index.php?title=Special:CiteThisPage&page=Solar_System&id=905437334
pub const SS_MASS: f64 = 1.0014;
/// Mass of the Sun
pub const SUN_GM: f64 = 132_712_440_041.939_38;

/// Enable or not light time correction for the computation of the celestial states
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LTCorr {
    /// No correction, i.e. assumes instantaneous propagation of photons
    None,
    /// Accounts for light-time correction. This is corresponds to CN in SPICE.
    LightTime,
    /// Accounts for light-time and stellar abberation where the solar system barycenter is the inertial frame. Corresponds to CN+S in SPICE.
    Abberation,
}

#[derive(Debug)]
pub struct FrameTree {
    name: String,
    frame: Frame,
    // If None, and has a parent (check frame.frame_path()), then rotation is I33 (and therefore no need to compute it)
    parent_rotation: Option<Box<dyn ParentRotation>>,
    children: Vec<FrameTree>,
}

impl FrameTree {
    /// Seek an ephemeris from its celestial name (e.g. Earth Moon Barycenter)
    fn frame_seek_by_name(
        name: &str,
        cur_path: &mut Vec<usize>,
        f: &FrameTree,
    ) -> Result<Vec<usize>, NyxError> {
        if f.name == name {
            Ok(cur_path.to_vec())
        } else if f.children.is_empty() {
            Err(NyxError::ObjectNotFound(name.to_string()))
        } else {
            for (cno, child) in f.children.iter().enumerate() {
                let mut this_path = cur_path.clone();
                this_path.push(cno);
                let child_attempt = Self::frame_seek_by_name(name, &mut this_path, child);
                if let Ok(found_path) = child_attempt {
                    return Ok(found_path);
                }
            }
            // Could not find name in iteration, fail
            Err(NyxError::ObjectNotFound(name.to_string()))
        }
    }
}

// Defines Cosm, from the Greek word for "world" or "universe".
pub struct Cosm {
    pub xb: Xb,
    pub frame_root: FrameTree,
    // Maps the ephemeris path to the frame root path (remove this with the upcoming xb file)
    ephem2frame_map: HashMap<Vec<usize>, Vec<usize>>,
}

impl fmt::Debug for Cosm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Cosm with `{}` as ephemerides root",
            match &self.xb.ephemeris_root {
                Some(r) => r.name.clone(),
                None => "NONE".to_string(),
            }
        )
    }
}

impl Cosm {
    /// Builds a Cosm from the *XB files. Path should _not_ contain file extension. Panics if the files could not be loaded.
    pub fn from_xb(filename: &str) -> Result<Self, NyxError> {
        Self::try_from_xb(Xb::from_file(filename)?)
    }

    /// Tries to load a subset of the DE438 XB from the embedded files, bounded between 01 Jan 2000 and 31 Dec 2050 TDB.
    pub fn try_de438() -> Result<Self, NyxError> {
        let de438_buf =
            EmbeddedAsset::get("de438s-00-50.xb").expect("Could not find asset de438s-00-550.xb");
        Self::try_from_xb(Xb::from_buffer(&de438_buf)?)
    }

    /// Load a subset of the DE438 XB from the embedded files, bounded between 01 Jan 2000 and 31 Dec 2050 TAI.
    pub fn de438() -> Arc<Self> {
        Arc::new(Self::try_de438().expect("could not load embedded de438s XB file"))
    }

    /// Load a subset of the DE438 XB from the embedded files, bounded between 01 Jan 2000 and 31 Dec 2050 TAI.
    pub fn de438_raw() -> Self {
        Self::try_de438().expect("could not load embedded de438s XB file")
    }

    /// Load a subset of the DE438 XB from the embedded files, bounded between 01 Jan 2000 and 31 Dec 2050 TAI.
    pub fn de438_gmat() -> Arc<Self> {
        let mut cosm = Self::try_de438().expect("could not load embedded de438s XB file");
        // Set all of the GMs and their body fixed frames too
        cosm.frame_mut_gm("Sun J2000", 132_712_440_017.99);
        cosm.frame_mut_gm("IAU Sun", 132_712_440_017.99);
        cosm.frame_mut_gm("Mercury Barycenter J2000", 22_032.080_486_418);
        // No IAU mercury
        cosm.frame_mut_gm("Venus Barycenter J2000", 324_858.598_826_46);
        cosm.frame_mut_gm("IAU Venus", 324_858.598_826_46);
        cosm.frame_mut_gm("EME2000", 398_600.441_5);
        cosm.frame_mut_gm("IAU Earth", 398_600.441_5);
        cosm.frame_mut_gm("Luna", 4_902.800_582_147_8);
        cosm.frame_mut_gm("IAU Moon", 4_902.800_582_147_8);
        cosm.frame_mut_gm("Mars Barycenter J2000", 42_828.314258067);
        cosm.frame_mut_gm("IAU Mars", 42_828.314258067);
        cosm.frame_mut_gm("Jupiter Barycenter J2000", 126_712_767.857_80);
        cosm.frame_mut_gm("IAU Jupiter", 126_712_767.857_80);
        cosm.frame_mut_gm("Saturn Barycenter J2000", 37_940_626.061_137);
        cosm.frame_mut_gm("IAU Saturn", 37_940_626.061_137);
        cosm.frame_mut_gm("Uranus Barycenter J2000", 5_794_549.007_071_9);
        cosm.frame_mut_gm("IAU Uranus", 5_794_549.007_071_9);
        cosm.frame_mut_gm("Neptune Barycenter J2000", 6_836_534.063_879_3);
        cosm.frame_mut_gm("IAU Neptune", 6_836_534.063_879_3);

        Arc::new(cosm)
    }

    /// Attempts to build a Cosm from the XB files and the embedded IAU frames
    pub fn try_from_xb(xb: Xb) -> Result<Self, NyxError> {
        let mut cosm = Cosm {
            xb,
            frame_root: FrameTree {
                name: "SSB J2000".to_string(),
                frame: Frame::Celestial {
                    axb_id: 0,
                    exb_id: 0,
                    gm: SS_MASS * SUN_GM,
                    parent_axb_id: None,
                    parent_exb_id: None,
                    ephem_path: [None, None, None],
                    frame_path: [None, None, None],
                },
                parent_rotation: None,
                children: Vec::new(),
            },
            ephem2frame_map: HashMap::new(),
        };
        cosm.append_xb();
        cosm.load_iau_frames()?;
        Ok(cosm)
    }

    /// Load the IAU Frames as defined in Celest Mech Dyn Astr (2018) 130:22 (https://doi.org/10.1007/s10569-017-9805-5)
    pub fn load_iau_frames(&mut self) -> Result<(), NyxError> {
        // Load the IAU frames from the embedded TOML
        let iau_toml_str =
            EmbeddedAsset::get("iau_frames.toml").expect("Could not find iau_frames.toml as asset");
        self.append_frames(
            std::str::from_utf8(&iau_toml_str)
                .expect("Could not deserialize iau_frames.toml as string"),
        )
    }

    /// Returns the machine path of the ephemeris whose orientation is requested
    pub fn frame_find_path_for_orientation(&self, name: &str) -> Result<Vec<usize>, NyxError> {
        if self.frame_root.name == name {
            // Return an empty vector (but OK because we're asking for the root)
            Ok(Vec::new())
        } else {
            let mut path = Vec::new();
            Self::frame_find_path(name, &mut path, &self.frame_root)
        }
    }

    /// Seek a frame from its orientation name
    fn frame_find_path(
        frame_name: &str,
        cur_path: &mut Vec<usize>,
        f: &FrameTree,
    ) -> Result<Vec<usize>, NyxError> {
        if f.name == frame_name {
            Ok(cur_path.to_vec())
        } else if f.children.is_empty() {
            Err(NyxError::ObjectNotFound(frame_name.to_string()))
        } else {
            for child in &f.children {
                let mut this_path = cur_path.clone();
                let child_attempt = Self::frame_find_path(frame_name, &mut this_path, child);
                if let Ok(found_path) = child_attempt {
                    return Ok(found_path);
                }
            }
            // Could not find name in iteration, fail
            Err(NyxError::ObjectNotFound(frame_name.to_string()))
        }
    }

    /// Returns the correct frame for this ephemeris
    fn default_frame_value(
        e: &Ephemeris,
        ephem_path: [Option<usize>; 3],
        pos: usize,
    ) -> Option<FrameTree> {
        match e.constants.get("GM") {
            Some(gm) => {
                // It's a geoid, and we assume everything else is there
                let flattening = match e.constants.get("Flattening") {
                    Some(param) => param.value,
                    None => {
                        if e.name == "Moon" {
                            0.0012
                        } else {
                            0.0
                        }
                    }
                };
                let equatorial_radius = match e.constants.get("Equatorial radius") {
                    Some(param) => param.value,
                    None => {
                        if e.name == "Moon" {
                            1738.1
                        } else {
                            0.0
                        }
                    }
                };
                let semi_major_radius = match e.constants.get("Equatorial radius") {
                    Some(param) => {
                        if e.name == "Earth Barycenter" {
                            6378.1370
                        } else {
                            param.value
                        }
                    }
                    None => equatorial_radius, // assume spherical if unspecified
                };

                // Let's now build the J2000 version of this body
                Some(FrameTree {
                    name: format!("{} J2000", e.name.clone()),
                    frame: Frame::Geoid {
                        gm: gm.value,
                        flattening,
                        equatorial_radius,
                        semi_major_radius,
                        axb_id: 0,
                        exb_id: 0,
                        parent_axb_id: None,
                        parent_exb_id: None,
                        ephem_path,
                        frame_path: [Some(pos), None, None],
                    },
                    parent_rotation: None,
                    children: Vec::new(),
                })
            }
            None => {
                if e.name.to_lowercase() == *"sun" {
                    // Build the Sun frame in J2000
                    Some(FrameTree {
                        name: "Sun J2000".to_string(),
                        frame: Frame::Geoid {
                            gm: SUN_GM,
                            flattening: 0.0,
                            // From https://iopscience.iop.org/article/10.1088/0004-637X/750/2/135
                            equatorial_radius: 696_342.0,
                            semi_major_radius: 696_342.0,
                            axb_id: 0,
                            exb_id: 0,
                            parent_axb_id: None,
                            parent_exb_id: None,
                            ephem_path,
                            frame_path: [Some(pos), None, None],
                        },
                        parent_rotation: None,
                        children: Vec::new(),
                    })
                } else {
                    warn!("no GM value for XB {}", e.name);
                    None
                }
            }
        }
    }

    pub fn append_xb(&mut self) {
        // Insert the links between the SSB ephem and the J2000 frame (data stored in self.frame_root!)
        self.ephem2frame_map.insert(Vec::new(), Vec::new());

        // Build the frames
        for i in 0..self.xb.ephemeris_root.as_ref().unwrap().children.len() {
            if let Ok(child) = self.xb.ephemeris_from_path(&[i]) {
                // Add base J2000 frame for all barycenters
                if let Some(frame) = Self::default_frame_value(
                    child,
                    [Some(i), None, None],
                    self.frame_root.children.len(),
                ) {
                    self.frame_root.children.push(frame);
                    let frame_path = vec![self.frame_root.children.len() - 1];
                    self.ephem2frame_map.insert(vec![i], frame_path);
                }

                // Try to go one level deeper
                for j in 0..child.children.len() {
                    if let Ok(next_child) = self.xb.ephemeris_from_path(&[i, j]) {
                        // Create the frame
                        if let Some(frame) = Self::default_frame_value(
                            next_child,
                            [Some(i), Some(j), None],
                            self.frame_root.children.len(),
                        ) {
                            // At this stage, they are all children of the J2000 frame
                            // Bug: This should eventually use the orientation of the XB or it'll fail if it isn't J2000 based
                            self.frame_root.children.push(frame);
                            let frame_path = vec![self.frame_root.children.len() - 1];
                            self.ephem2frame_map.insert(vec![i, j], frame_path);
                        }
                    }
                }
            }
        }
    }

    /// Append Cosm with the contents of this TOML (must _not_ be the filename)
    pub fn append_frames(&mut self, toml_content: &str) -> Result<(), NyxError> {
        let maybe_frames: Result<frame_serde::FramesSerde, _> = toml::from_str(toml_content);
        match maybe_frames {
            Ok(mut frames) => {
                for (ref name, ref mut definition) in frames.frames.drain() {
                    if self.try_frame(name.as_str()).is_ok() {
                        warn!("overwriting frame `{}`", name);
                    }
                    if let Some(src_frame_name) = &definition.inherit {
                        match self.try_frame(src_frame_name.as_str()) {
                            Ok(src_frame) => {
                                definition.update_from(&src_frame);
                            }
                            Err(_) => error!(
                                "frame `{}` is derived from unknown frame `{}`, skipping!",
                                name, src_frame_name
                            ),
                        }
                    }
                    let rot = &definition.rotation;
                    let right_asc: Expr = match rot.right_asc.parse() {
                        Ok(expr) => expr,
                        Err(e) => {
                            let msg = format!("[frame.{}] - could not parse right_asc `{}` - are there any special characters? {}",
                            &name, &rot.right_asc, e);
                            error!("{}", msg);
                            return Err(NyxError::LoadingError(msg));
                        }
                    };
                    let declin: Expr = match rot.declin.parse() {
                        Ok(expr) => expr,
                        Err(e) => {
                            let msg = format!("[frame.{}] - could not parse declin `{}` - are there any special characters? {}",
                            &name, &rot.declin, e);
                            error!("{}", msg);
                            return Err(NyxError::LoadingError(msg));
                        }
                    };
                    let w_expr: Expr = match rot.w.parse() {
                        Ok(expr) => expr,
                        Err(e) => {
                            let msg = format!("[frame.{}] - could not parse w `{}` - are there any special characters? {}",
                            &name, &rot.w, e);
                            error!("{}", msg);
                            return Err(NyxError::LoadingError(msg));
                        }
                    };

                    let frame_rot = Euler3AxisDt::from_ra_dec_w(
                        right_asc,
                        declin,
                        w_expr,
                        match &rot.context {
                            Some(ctx) => ctx.clone(),
                            None => HashMap::new(),
                        },
                        match &rot.angle_unit {
                            Some(val) => AngleUnit::from_str(val.as_str()).unwrap(),
                            None => AngleUnit::Degrees,
                        },
                    );

                    // Let's now create the Frame, we'll add the ephem path and frame path just after
                    let mut new_frame = definition.as_frame();
                    let frame_name = name.replace("_", " ").trim().to_string();

                    // Grab the inherited frame again so we know how to place it in the frame tree
                    if let Some(src_frame_name) = &definition.inherit {
                        debug!("Loaded frame {}", frame_name);
                        let src_frame = self.try_frame(src_frame_name.as_str()).unwrap();
                        let mut fpath = src_frame.frame_path();
                        // And find the correct children
                        let children = match fpath.len() {
                            2 => {
                                &mut self.frame_root.children[fpath[0]].children[fpath[1]].children
                            }
                            1 => &mut self.frame_root.children[fpath[0]].children,
                            0 => &mut self.frame_root.children,
                            _ => unimplemented!("Too many children for now"),
                        };
                        fpath.push(children.len());
                        // Set the frame path and ephem path for this new frame
                        match new_frame {
                            Frame::Celestial {
                                ref mut ephem_path,
                                ref mut frame_path,
                                ..
                            }
                            | Frame::Geoid {
                                ref mut ephem_path,
                                ref mut frame_path,
                                ..
                            } => {
                                match fpath.len() {
                                    3 => {
                                        *frame_path =
                                            [Some(fpath[0]), Some(fpath[1]), Some(fpath[2])]
                                    }
                                    2 => *frame_path = [Some(fpath[0]), Some(fpath[1]), None],
                                    1 => *frame_path = [Some(fpath[0]), None, None],
                                    _ => unimplemented!(),
                                };

                                let epath = src_frame.ephem_path();
                                match epath.len() {
                                    3 => {
                                        *ephem_path =
                                            [Some(epath[0]), Some(epath[1]), Some(epath[2])]
                                    }
                                    2 => *ephem_path = [Some(epath[0]), Some(epath[1]), None],
                                    1 => *ephem_path = [Some(epath[0]), None, None],
                                    _ => unimplemented!(),
                                };
                            }
                            _ => unimplemented!(),
                        }

                        // And create and insert
                        // Create the new FrameTree node, and insert it as a child of the current path
                        let fnode = FrameTree {
                            name: frame_name,
                            frame: new_frame,
                            parent_rotation: Some(Box::new(frame_rot)),
                            children: Vec::new(),
                        };

                        children.push(fnode);
                    } else {
                        warn!(
                            "Frame `{}` does not inherit from anyone, cannot organize tree",
                            frame_name
                        );
                    }
                }
                Ok(())
            }
            Err(e) => {
                error!("{}", e);
                Err(NyxError::LoadingError(format!("{}", e)))
            }
        }
    }

    /// Returns the expected frame name with its ephemeris name for querying
    fn fix_frame_name(name: &str) -> String {
        let name = name.to_lowercase().trim().replace("_", " ");
        // Handle the specific frames
        if name == "eme2000" {
            String::from("Earth J2000")
        } else if name == "luna" {
            String::from("Moon J2000")
        } else if name == "earth moon barycenter" {
            String::from("Earth Barycenter J2000")
        } else if name == "ssb" {
            String::from("SSB J2000")
        } else {
            let splt: Vec<_> = name.split(' ').collect();
            if splt[0] == "iau" {
                // This is an IAU frame, so the orientation is specified first, and we don't capitalize the ephemeris name
                vec![splt[0].to_string(), splt[1..splt.len()].join(" ")].join(" ")
            } else {
                // Likely a default center and frame, so let's do some clever guessing and capitalize the words
                let frame_name = capitalize(&splt[splt.len() - 1].to_string());
                let ephem_name = splt[0..splt.len() - 1]
                    .iter()
                    .map(|word| capitalize(word))
                    .collect::<Vec<_>>()
                    .join(" ");

                vec![ephem_name, frame_name].join(" ")
            }
        }
    }

    /// Fetch the frame associated with this ephemeris name
    /// This is slow, so avoid using it.
    pub fn try_frame(&self, name: &str) -> Result<Frame, NyxError> {
        let name = Self::fix_frame_name(name);
        if self.frame_root.name == name {
            // Return an empty vector (but OK because we're asking for the root)
            Ok(self.frame_root.frame)
        } else {
            let mut path = Vec::new();
            Ok(self.frame_from_frame_path(&FrameTree::frame_seek_by_name(
                &name,
                &mut path,
                &self.frame_root,
            )?))
        }
    }

    /// Provided an ephemeris path and an optional frame name, returns the Frame of that ephemeris.
    /// For example, if [3, 1] is provided (Moon in J2000 in the DE file), return Moon J2000
    /// If no frame name is provided, then the storage frame is returned. Otherwise, the correct frame is returned.
    pub fn frame_from_ephem_path(&self, ephem_path: &[usize]) -> Frame {
        self.frame_from_frame_path(self.ephem2frame_map.get(&ephem_path.to_vec()).unwrap())
    }

    /// Provided a frame path returns the Frame.
    pub fn frame_from_frame_path(&self, frame_path: &[usize]) -> Frame {
        match frame_path.len() {
            2 => self.frame_root.children[frame_path[0]].children[frame_path[1]].frame,
            1 => self.frame_root.children[frame_path[0]].frame,
            0 => self.frame_root.frame,
            _ => unimplemented!("Not expecting three layers of attitude frames"),
        }
    }

    fn frame_names(mut names: &mut Vec<String>, f: &FrameTree) {
        names.push(f.name.clone());
        for child in &f.children {
            Self::frame_names(&mut names, child);
        }
    }

    pub fn frames_get_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        Self::frame_names(&mut names, &self.frame_root);
        names
    }

    /// Returns the geoid from the loaded XB, if it is in there, else panics!
    pub fn frame(&self, name: &str) -> Frame {
        self.try_frame(name).unwrap()
    }

    /// Mutates the GM value for the provided geoid id. Panics if ID not found.
    pub fn frame_mut_gm(&mut self, name: &str, new_gm: f64) {
        // Grab the frame -- this may panic!
        let frame_path = self.frame(name).frame_path();

        match frame_path.len() {
            2 => self.frame_root.children[frame_path[0]].children[frame_path[1]]
                .frame
                .gm_mut(new_gm),
            1 => self.frame_root.children[frame_path[0]].frame.gm_mut(new_gm),
            0 => self.frame_root.frame.gm_mut(new_gm),
            _ => unimplemented!("Not expecting three layers of attitude frames"),
        }
    }

    /// Returns the celestial state as computed from a de4xx.{FXB,XB} file in the original frame
    pub fn raw_celestial_state(&self, path: &[usize], epoch: Epoch) -> Result<Orbit, NyxError> {
        if path.is_empty() {
            // This is the solar system barycenter, so we just return a state of zeros
            return Ok(Orbit::cartesian(
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                epoch,
                self.frame_root.frame,
            ));
        }
        let ephem = self.xb.ephemeris_from_path(path)?;

        // Compute the position as per the algorithm from jplephem
        let interp = ephem
            .interpolator
            .as_ref()
            .ok_or_else(|| NyxError::NoInterpolationData(ephem.name.clone()))?;

        // the DE file epochs are all in ET mod julian
        let start_mod_julian_f64 = ephem.start_epoch.as_ref().unwrap().as_raw();
        let coefficient_count: usize = interp.position_degree as usize;
        if coefficient_count <= 2 {
            // Cf. https://gitlab.com/chrisrabotin/nyx/-/issues/131
            return Err(NyxError::InvalidInterpolationData(format!(
                "position_degree is less than 3 for XB {}",
                ephem.name
            )));
        }

        let exb_states = match interp
            .state_data
            .as_ref()
            .ok_or_else(|| NyxError::NoStateData(ephem.name.clone()))?
        {
            EqualStates(states) => states,
            VarwindowStates(_) => panic!("variable window not yet supported by Cosm"),
        };

        let interval_length: f64 = exb_states.window_duration;

        let epoch_jde = epoch.as_jde_tdb_days();
        let delta_jde = epoch_jde - start_mod_julian_f64;

        let index_f = (delta_jde / interval_length).floor();
        let mut offset = delta_jde - index_f * interval_length;
        let mut index = index_f as usize;
        if index == exb_states.position.len() {
            index -= 1;
            offset = interval_length;
        }
        let pos_coeffs = &exb_states.position[index];

        let mut interp_t = vec![0.0; coefficient_count];
        let t1 = 2.0 * offset / interval_length - 1.0;
        interp_t[0] = 1.0;
        interp_t[1] = t1;
        for i in 2..coefficient_count {
            interp_t[i] = (2.0 * t1) * interp_t[i - 1] - interp_t[i - 2];
        }

        let mut interp_dt = vec![0.0; coefficient_count];
        interp_dt[0] = 0.0;
        interp_dt[1] = 1.0;
        interp_dt[2] = 2.0 * (2.0 * t1);
        for i in 3..coefficient_count {
            interp_dt[i] = (2.0 * t1) * interp_dt[i - 1] - interp_dt[i - 2]
                + interp_t[i - 1]
                + interp_t[i - 1];
        }
        for interp_i in &mut interp_dt {
            *interp_i *= 2.0 / interval_length;
        }

        let mut x = 0.0;
        let mut y = 0.0;
        let mut z = 0.0;
        let mut vx = 0.0;
        let mut vy = 0.0;
        let mut vz = 0.0;

        for (idx, pos_factor) in interp_t.iter().enumerate() {
            let vel_factor = interp_dt[idx];
            x += pos_factor * pos_coeffs.x[idx];
            y += pos_factor * pos_coeffs.y[idx];
            z += pos_factor * pos_coeffs.z[idx];
            vx += vel_factor * pos_coeffs.x[idx];
            vy += vel_factor * pos_coeffs.y[idx];
            vz += vel_factor * pos_coeffs.z[idx];
        }

        // Get the Geoid associated with the ephemeris frame
        let storage_geoid = self.frame_from_ephem_path(path);
        Ok(Orbit::cartesian(
            x,
            y,
            z,
            vx / SECONDS_PER_DAY,
            vy / SECONDS_PER_DAY,
            vz / SECONDS_PER_DAY,
            epoch,
            storage_geoid,
        ))
    }

    /// Attempts to return the state of the celestial object of XB ID `exb_id` (the target) at time `jde` `as_seen_from`
    ///
    /// The light time correction is based on SPICE's implementation: https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/C/cspice/spkezr_c.html .
    /// Aberration computation is a conversion of the stelab function in SPICE, available here
    /// https://github.com/ChristopherRabotin/cspice/blob/26c72936fb7ff6f366803a1419b7cc3c61e0b6e5/src/cspice/stelab.c#L255
    pub fn try_celestial_state(
        &self,
        target_ephem: &[usize],
        datetime: Epoch,
        frame: Frame,
        correction: LTCorr,
    ) -> Result<Orbit, NyxError> {
        let target_frame = self.frame_from_ephem_path(target_ephem);
        match correction {
            LTCorr::None => {
                // let target_frame = self.try_frame_by_exb_id(target_exb_id)?;
                let state = Orbit::cartesian(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, datetime, target_frame);
                Ok(-self.try_frame_chg(&state, frame)?)
            }
            LTCorr::LightTime | LTCorr::Abberation => {
                // Get the geometric states as seen from SSB
                let ssb2k = self.frame_root.frame;

                let obs =
                    self.try_celestial_state(&frame.ephem_path(), datetime, ssb2k, LTCorr::None)?;
                let mut tgt =
                    self.try_celestial_state(target_ephem, datetime, ssb2k, LTCorr::None)?;
                // It will take less than three iterations to converge
                for _ in 0..3 {
                    // Compute the light time
                    let lt = (tgt - obs).rmag() / SPEED_OF_LIGHT_KMS;
                    // Compute the new target state
                    let lt_dt = datetime - lt * TimeUnit::Second;
                    tgt = self
                        .try_celestial_state(target_ephem, lt_dt, ssb2k, LTCorr::None)
                        .unwrap();
                }
                // Compute the correct state
                let mut state = Orbit::cartesian(
                    (tgt - obs).x,
                    (tgt - obs).y,
                    (tgt - obs).z,
                    (tgt - obs).vx,
                    (tgt - obs).vy,
                    (tgt - obs).vz,
                    datetime,
                    frame,
                );

                // Incluee the range-rate term in the velocity computation as explained in
                // https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/C/req/abcorr.html#Reception%20case
                let state_acc = state.velocity() / state.rmag();
                let dltdt = state.radius().dot(&state_acc) / SPEED_OF_LIGHT_KMS;

                state.vx = tgt.vx * (1.0 - dltdt) - obs.vx;
                state.vy = tgt.vy * (1.0 - dltdt) - obs.vy;
                state.vz = tgt.vz * (1.0 - dltdt) - obs.vz;

                if correction == LTCorr::Abberation {
                    // Get a unit vector that points in the direction of the object
                    let r_hat = state.r_hat();
                    // Get the velocity vector (of the observer) scaled with respect to the speed of light
                    let vbyc = obs.velocity() / SPEED_OF_LIGHT_KMS;
                    /* If the square of the length of the velocity vector is greater than or equal
                    to one, the speed of the observer is greater than or equal to the speed of light.
                    The observer speed is definitely out of range. */
                    if vbyc.dot(&vbyc) >= 1.0 {
                        warn!("observer is traveling faster than the speed of light");
                    } else {
                        let h_hat = r_hat.cross(&vbyc);
                        /* If the magnitude of the vector H is zero, the observer is moving along the line
                        of sight to the object, and no correction is required. Otherwise, rotate the
                        position of the object by phi radians about H to obtain the apparent position. */
                        if h_hat.norm() > std::f64::EPSILON {
                            let phi = h_hat.norm().asin();
                            let ab_pos = rotv(&state.radius(), &h_hat, phi);
                            state.x = ab_pos[0];
                            state.y = ab_pos[1];
                            state.z = ab_pos[2];
                        }
                    }
                }
                Ok(state)
            }
        }
    }

    /// Returns the state of the celestial object of XB ID `exb_id` (the target) at time `jde` `as_seen_from`, or panics
    pub fn celestial_state(
        &self,
        target_ephem: &[usize],
        datetime: Epoch,
        frame: Frame,
        correction: LTCorr,
    ) -> Orbit {
        self.try_celestial_state(target_ephem, datetime, frame, correction)
            .unwrap()
    }

    /// Return the DCM to go from the `from` frame to the `to` frame
    pub fn try_frame_chg_dcm_from_to(
        &self,
        from: &Frame,
        to: &Frame,
        dt: Epoch,
    ) -> Result<Matrix3<f64>, NyxError> {
        // And now let's compute the rotation path
        let mut dcm = Matrix3::<f64>::identity();

        if to.frame_path() == from.frame_path() {
            // No need to go any further
            return Ok(dcm);
        }

        let state_frame_path = from.frame_path();
        let new_frame_path = to.frame_path();

        // Let's get the translation path between both both states.
        let f_common_path = self.find_common_root(&new_frame_path, &state_frame_path)?;

        let get_dcm = |path: &[usize]| -> &FrameTree {
            // This is absolutely terrible, and there must be a better way to do it, but it's late.
            match path.len() {
                1 => &self.frame_root.children[path[0]],
                2 => &self.frame_root.children[path[0]].children[path[1]],
                3 => &self.frame_root.children[path[0]].children[path[1]].children[path[2]],
                _ => unimplemented!(),
            }
        };

        let mut negated_fwd = false;
        // Walk forward from the destination state
        for i in (f_common_path.len()..new_frame_path.len()).rev() {
            if let Some(parent_rot) = &get_dcm(&new_frame_path[0..=i]).parent_rotation {
                if let Some(next_dcm) = parent_rot.dcm_to_parent(dt) {
                    if new_frame_path.len() < state_frame_path.len() && i == f_common_path.len() {
                        dcm *= next_dcm.transpose();
                        negated_fwd = true;
                    } else {
                        dcm *= next_dcm;
                    }
                }
            }
        }
        // Walk backward from current state up to common node
        for i in (f_common_path.len()..state_frame_path.len()).rev() {
            if let Some(parent_rot) = &get_dcm(&state_frame_path[0..=i]).parent_rotation {
                if let Some(next_dcm) = parent_rot.dcm_to_parent(dt) {
                    if !negated_fwd && i == f_common_path.len() {
                        // We just crossed the common point, so let's negate this state
                        dcm *= next_dcm.transpose();
                    } else {
                        dcm *= next_dcm;
                    }
                }
            }
        }

        if negated_fwd {
            dcm = dcm.transpose();
        }

        Ok(dcm)
    }

    /// Attempts to return the provided state in the provided frame.
    pub fn try_frame_chg(&self, state: &Orbit, new_frame: Frame) -> Result<Orbit, NyxError> {
        if state.frame == new_frame {
            return Ok(*state);
        }
        let new_ephem_path = new_frame.ephem_path();
        let state_ephem_path = state.frame.ephem_path();

        // Let's get the translation path between both both states.
        let e_common_path = self.find_common_root(&new_ephem_path, &state_ephem_path)?;

        // This doesn't make sense, but somehow the following algorithm only works when converting spacecraft states
        let mut new_state = if state.rmag() > 0.0 {
            let mut new_state = *state;
            // Walk backward from current state up to common node
            for i in (e_common_path.len()..state_ephem_path.len()).rev() {
                let next_state = self.raw_celestial_state(&state_ephem_path[0..=i], state.dt)?;
                new_state = new_state + next_state;
            }

            // Walk forward from the destination state
            for i in (e_common_path.len()..new_ephem_path.len()).rev() {
                let next_state = self.raw_celestial_state(&new_ephem_path[0..=i], state.dt)?;
                new_state = new_state - next_state;
            }

            new_state
        } else {
            let mut negated_fwd = false;

            let mut new_state = if state_ephem_path.is_empty() {
                // SSB, let's invert this
                -*state
            } else {
                *state
            };

            // Walk forward from the destination state
            for i in (e_common_path.len()..new_ephem_path.len()).rev() {
                let next_state = self.raw_celestial_state(&new_ephem_path[0..=i], state.dt)?;
                if new_ephem_path.len() < state_ephem_path.len() && i == e_common_path.len() {
                    // We just crossed the common point going forward, so let's add the opposite of this state
                    new_state = new_state - next_state;
                    negated_fwd = true;
                } else {
                    new_state = new_state + next_state;
                }
            }
            // Walk backward from current state up to common node
            for i in (e_common_path.len()..state_ephem_path.len()).rev() {
                let next_state = self.raw_celestial_state(&state_ephem_path[0..=i], state.dt)?;
                if !negated_fwd && i == e_common_path.len() {
                    // We just crossed the common point (and haven't passed it going forward), so let's negate this state
                    new_state = new_state - next_state;
                } else {
                    new_state = new_state + next_state;
                }
            }

            if negated_fwd {
                // Because we negated the state going forward, let's flip it back to its correct orientation now.
                -new_state
            } else {
                new_state
            }
        };

        new_state.frame = new_frame;

        // And now let's compute the rotation path
        new_state.apply_dcm(self.try_frame_chg_dcm_from_to(&state.frame, &new_frame, state.dt)?);
        Ok(new_state)
    }

    /// Return the provided state in the provided frame, or panics
    pub fn frame_chg(&self, state: &Orbit, new_frame: Frame) -> Orbit {
        self.try_frame_chg(state, new_frame).unwrap()
    }

    /// Returns the conversion path from the target ephemeris or frame `from` as seen from `to`.
    fn find_common_root(&self, from: &[usize], to: &[usize]) -> Result<Vec<usize>, NyxError> {
        let mut common_root = Vec::with_capacity(3); // Unlikely to be more than 3 items
        if from.is_empty() || to.is_empty() {
            // It will necessarily be the root of the ephemeris
            Ok(common_root)
        } else {
            if from.len() < to.len() {
                // Iterate through the items in from
                for (n, obj) in from.iter().enumerate() {
                    if &to[n] == obj {
                        common_root.push(*obj);
                    } else {
                        // Found the end of the matching objects
                        break;
                    }
                }
            } else {
                // Iterate through the items in from
                for (n, obj) in to.iter().enumerate() {
                    if &from[n] == obj {
                        common_root.push(*obj);
                    } else {
                        // Found the end of the matching objects
                        break;
                    }
                }
            }
            Ok(common_root)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celestia::Bodies;

    /// Tests direct transformations. Test cases generated via jplephem, hence the EPSILON precision.
    /// Note however that there is a difference between jplephem and spiceypy, cf.
    /// https://github.com/brandon-rhodes/python-jplephem/issues/33
    #[test]
    fn test_cosm_direct() {
        use std::f64::EPSILON;
        let cosm = Cosm::de438();

        let eb_frame = cosm.frame(&Bodies::EarthBarycenter.name());

        assert_eq!(eb_frame.ephem_path(), Bodies::EarthBarycenter.ephem_path());

        assert_eq!(
            cosm.find_common_root(Bodies::Earth.ephem_path(), Bodies::Earth.ephem_path())
                .unwrap()
                .len(),
            2,
            "Conversions within Earth does not require any translation"
        );

        let jde = Epoch::from_jde_et(2_452_312.5);
        let c = LTCorr::None;

        let earth_bary2k = cosm.frame("Earth Barycenter J2000");
        let ssb2k = cosm.frame("SSB");
        let earth_moon2k = cosm.frame("Luna");

        assert!(
            cosm.celestial_state(Bodies::EarthBarycenter.ephem_path(), jde, earth_bary2k, c)
                .rmag()
                < EPSILON
        );

        let out_state = cosm.celestial_state(Bodies::EarthBarycenter.ephem_path(), jde, ssb2k, c);
        assert_eq!(out_state.frame.ephem_path(), vec![]);
        assert!((out_state.x - -109_837_695.021_661_42).abs() < 1e-13);
        assert!((out_state.y - 89_798_622.194_651_56).abs() < 1e-13);
        assert!((out_state.z - 38_943_878.275_922_61).abs() < 1e-13);
        assert!((out_state.vx - -20.400_327_981_451_596).abs() < 1e-13);
        assert!((out_state.vy - -20.413_134_121_084_312).abs() < 1e-13);
        assert!((out_state.vz - -8.850_448_420_104_028).abs() < 1e-13);

        // And the opposite transformation
        let out_state = cosm.celestial_state(Bodies::SSB.ephem_path(), jde, earth_bary2k, c);
        assert_eq!(
            out_state.frame.ephem_path(),
            Bodies::EarthBarycenter.ephem_path()
        );
        assert!((out_state.x - 109_837_695.021_661_42).abs() < 1e-13);
        assert!((out_state.y - -89_798_622.194_651_56).abs() < 1e-13);
        assert!((out_state.z - -38_943_878.275_922_61).abs() < 1e-13);
        assert!((out_state.vx - 20.400_327_981_451_596).abs() < 1e-13);
        assert!((out_state.vy - 20.413_134_121_084_312).abs() < 1e-13);
        assert!((out_state.vz - 8.850_448_420_104_028).abs() < 1e-13);

        let out_state =
            cosm.celestial_state(Bodies::EarthBarycenter.ephem_path(), jde, earth_moon2k, c);
        assert_eq!(out_state.frame.ephem_path(), Bodies::Luna.ephem_path());
        assert!((out_state.x - 81_638.253_069_843_03).abs() < 1e-9);
        assert!((out_state.y - 345_462.617_249_631_9).abs() < 1e-9);
        assert!((out_state.z - 144_380.059_413_586_45).abs() < 1e-9);
        assert!((out_state.vx - -0.960_674_300_894_127_2).abs() < 1e-12);
        assert!((out_state.vy - 0.203_736_475_764_411_6).abs() < 1e-12);
        assert!((out_state.vz - 0.183_869_552_742_917_6).abs() < 1e-12);
        // Add the reverse test too
        let out_state = cosm.celestial_state(Bodies::Luna.ephem_path(), jde, earth_bary2k, c);
        assert_eq!(
            out_state.frame.ephem_path(),
            Bodies::EarthBarycenter.ephem_path()
        );
        assert!((out_state.x - -81_638.253_069_843_03).abs() < 1e-10);
        assert!((out_state.y - -345_462.617_249_631_9).abs() < 1e-10);
        assert!((out_state.z - -144_380.059_413_586_45).abs() < 1e-10);
        assert!((out_state.vx - 0.960_674_300_894_127_2).abs() < EPSILON);
        assert!((out_state.vy - -0.203_736_475_764_411_6).abs() < EPSILON);
        assert!((out_state.vz - -0.183_869_552_742_917_6).abs() < EPSILON);

        // The following test case comes from jplephem loaded with de438s.bsp
        let out_state = cosm.celestial_state(Bodies::Sun.ephem_path(), jde, ssb2k, c);
        assert_eq!(out_state.frame.ephem_path(), Bodies::SSB.ephem_path());
        assert!((out_state.x - -182_936.040_274_732_14).abs() < EPSILON);
        assert!((out_state.y - -769_329.776_328_230_7).abs() < EPSILON);
        assert!((out_state.z - -321_490.795_782_183_1).abs() < EPSILON);
        assert!((out_state.vx - 0.014_716_178_620_115_785).abs() < EPSILON);
        assert!((out_state.vy - 0.001_242_263_392_603_425).abs() < EPSILON);
        assert!((out_state.vz - 0.000_134_043_776_253_089_48).abs() < EPSILON);

        // And the opposite transformation
        let out_state =
            cosm.celestial_state(Bodies::SSB.ephem_path(), jde, cosm.frame("Sun J2000"), c);
        assert_eq!(out_state.frame.ephem_path(), Bodies::Sun.ephem_path());
        assert!((out_state.x - 182_936.040_274_732_14).abs() < EPSILON);
        assert!((out_state.y - 769_329.776_328_230_7).abs() < EPSILON);
        assert!((out_state.z - 321_490.795_782_183_1).abs() < EPSILON);
        assert!((out_state.vx - -0.014_716_178_620_115_785).abs() < EPSILON);
        assert!((out_state.vy - -0.001_242_263_392_603_425).abs() < EPSILON);
        assert!((out_state.vz - -0.000_134_043_776_253_089_48).abs() < EPSILON);

        let out_state = cosm.celestial_state(Bodies::Earth.ephem_path(), jde, earth_bary2k, c);
        assert_eq!(
            out_state.frame.ephem_path(),
            Bodies::EarthBarycenter.ephem_path()
        );
        assert!((out_state.x - 1_004.153_534_699_454_6).abs() < EPSILON);
        assert!((out_state.y - 4_249.202_979_894_305).abs() < EPSILON);
        assert!((out_state.z - 1_775.880_075_192_657_8).abs() < EPSILON);
        assert!((out_state.vx - -0.011_816_329_461_539_0).abs() < EPSILON);
        assert!((out_state.vy - 0.002_505_966_193_458_6).abs() < EPSILON);
        assert!((out_state.vz - 0.002_261_602_304_895_6).abs() < EPSILON);

        // And the opposite transformation
        let out_state = cosm.celestial_state(
            Bodies::EarthBarycenter.ephem_path(),
            jde,
            cosm.frame("EME2000"),
            c,
        );
        assert_eq!(out_state.frame.ephem_path(), Bodies::Earth.ephem_path());
        assert!((out_state.x - -1_004.153_534_699_454_6).abs() < EPSILON);
        assert!((out_state.y - -4_249.202_979_894_305).abs() < EPSILON);
        assert!((out_state.z - -1_775.880_075_192_657_8).abs() < EPSILON);
        assert!((out_state.vx - 0.011_816_329_461_539_0).abs() < EPSILON);
        assert!((out_state.vy - -0.002_505_966_193_458_6).abs() < EPSILON);
        assert!((out_state.vz - -0.002_261_602_304_895_6).abs() < EPSILON);
    }

    #[test]
    fn test_cosm_indirect() {
        use crate::utils::is_diagonal;
        use std::f64::EPSILON;

        let jde = Epoch::from_gregorian_utc_at_midnight(2002, 2, 7);

        let cosm = Cosm::de438();

        let ven2ear = cosm
            .find_common_root(Bodies::Venus.ephem_path(), Bodies::Luna.ephem_path())
            .unwrap();
        assert_eq!(
            ven2ear.len(),
            0,
            "Venus -> (SSB) -> Earth Barycenter -> Earth Moon, therefore common root is zero lengthed"
        );

        // In this first part of the tests, we want to check that the DCM corresponds to no rotation, or none.
        // If the DCM is diagonal, then it does not have a rotation.

        assert!(
            is_diagonal(
                &cosm
                    .try_frame_chg_dcm_from_to(
                        &cosm.frame("Earth Barycenter J2000"),
                        &cosm.frame("Earth Barycenter J2000"),
                        jde
                    )
                    .unwrap()
            ),
            "Conversion does not require any rotation"
        );

        assert!(
            is_diagonal(
                &cosm
                    .try_frame_chg_dcm_from_to(
                        &cosm.frame("Venus Barycenter J2000"),
                        &cosm.frame("EME2000"),
                        jde
                    )
                    .unwrap()
            ),
            "Conversion does not require any rotation"
        );

        assert!(
            !is_diagonal(
                &cosm
                    .try_frame_chg_dcm_from_to(
                        &cosm.frame("Venus Barycenter J2000"),
                        &cosm.frame("IAU Sun"),
                        jde
                    )
                    .unwrap()
            ),
            "Conversion to Sun IAU from Venus J2k requires one rotation"
        );

        assert!(
            !is_diagonal(
                &cosm
                    .try_frame_chg_dcm_from_to(
                        &cosm.frame("IAU Sun"),
                        &cosm.frame("Venus Barycenter J2000"),
                        jde
                    )
                    .unwrap()
            ),
            "Conversion to Sun IAU from Venus J2k requires one rotation"
        );

        let c = LTCorr::None;
        // Error is sometimes up to 0.6 meters!
        // I think this is related to https://github.com/brandon-rhodes/python-jplephem/issues/33
        let tol_pos = 7e-4; // km
        let tol_vel = 5e-7; // km/s

        /*
        # Preceed all of the following python examples with
        >>> import spiceypy as sp
        >>> sp.furnsh('bsp/de438s.bsp')
        >>> et = 66312064.18493939
        */

        let earth_moon = cosm.frame("Luna");
        let ven2ear_state =
            cosm.celestial_state(Bodies::VenusBarycenter.ephem_path(), jde, earth_moon, c);
        assert_eq!(ven2ear_state.frame.ephem_path(), Bodies::Luna.ephem_path());
        /*
        >>> ['{:.16e}'.format(x) for x in sp.spkez(1, et, "J2000", "NONE", 301)[0]]
        ['2.0512621957200775e+08', '-1.3561254792308527e+08', '-6.5578399676151529e+07', '3.6051374278177832e+01', '4.8889024622170766e+01', '2.0702933800843084e+01']
        */
        // NOTE: Venus position is quite off, not sure why.
        assert!(dbg!(ven2ear_state.x - 2.051_262_195_720_077_5e8).abs() < 7e-4);
        assert!(dbg!(ven2ear_state.y - -1.356_125_479_230_852_7e8).abs() < 7e-4);
        assert!(dbg!(ven2ear_state.z - -6.557_839_967_615_153e7).abs() < 7e-4);
        assert!(dbg!(ven2ear_state.vx - 3.605_137_427_817_783e1).abs() < 10.0 * tol_vel);
        assert!(dbg!(ven2ear_state.vy - 4.888_902_462_217_076_6e1).abs() < 10.0 * tol_vel);
        assert!(dbg!(ven2ear_state.vz - 2.070_293_380_084_308_4e1).abs() < 10.0 * tol_vel);

        // Check that conversion via a center frame works
        let earth_bary = cosm.frame("Earth Barycenter J2000");
        let moon_from_emb = cosm.celestial_state(Bodies::Luna.ephem_path(), jde, earth_bary, c);
        // Check this state again, as in the direct test
        /*
        >>> ['{:.16e}'.format(x) for x in sp.spkez(301, et, "J2000", "NONE", 3)[0]]
        ['-8.1576591043050896e+04', '-3.4547568914480874e+05', '-1.4439185901465410e+05', '9.6071184439702662e-01', '-2.0358322542180365e-01', '-1.8380551745739407e-01']
        */
        assert_eq!(moon_from_emb.frame, earth_bary);
        assert!(dbg!(moon_from_emb.x - -8.157_659_104_305_09e4).abs() < tol_pos);
        assert!(dbg!(moon_from_emb.y - -3.454_756_891_448_087_4e5).abs() < tol_pos);
        assert!(dbg!(moon_from_emb.z - -1.443_918_590_146_541e5).abs() < tol_pos);
        assert!(dbg!(moon_from_emb.vx - 9.607_118_443_970_266e-1).abs() < tol_vel);
        assert!(dbg!(moon_from_emb.vy - -2.035_832_254_218_036_5e-1).abs() < tol_vel);
        assert!(dbg!(moon_from_emb.vz - -1.838_055_174_573_940_7e-1).abs() < tol_vel);

        let earth_from_emb = cosm.celestial_state(Bodies::Earth.ephem_path(), jde, earth_bary, c);
        /*
        >>> ['{:.16e}'.format(x) for x in sp.spkez(399, et, "J2000", "NONE", 3)[0]]
        ['1.0033950894874154e+03', '4.2493637646888546e+03', '1.7760252107225667e+03', '-1.1816791248014408e-02', '2.5040812085717632e-03', '2.2608146685133296e-03']
        */
        assert!((earth_from_emb.x - 1.003_395_089_487_415_4e3).abs() < tol_pos);
        assert!((earth_from_emb.y - 4.249_363_764_688_855e3).abs() < tol_pos);
        assert!((earth_from_emb.z - 1.776_025_210_722_566_7e3).abs() < tol_pos);
        assert!((earth_from_emb.vx - -1.181_679_124_801_440_8e-2).abs() < tol_vel);
        assert!((earth_from_emb.vy - 2.504_081_208_571_763e-3).abs() < tol_vel);
        assert!((earth_from_emb.vz - 2.260_814_668_513_329_6e-3).abs() < tol_vel);

        let eme2k = cosm.frame("EME2000");
        let moon_from_earth = cosm.celestial_state(Bodies::Luna.ephem_path(), jde, eme2k, c);
        let earth_from_moon = cosm.celestial_state(Bodies::Earth.ephem_path(), jde, earth_moon, c);

        assert_eq!(earth_from_moon.radius(), -moon_from_earth.radius());
        assert_eq!(earth_from_moon.velocity(), -moon_from_earth.velocity());

        /*
        >>> ['{:.16e}'.format(x) for x in sp.spkez(301, et, "J2000", "NONE", 399)[0]]
        ['-8.2579986132538310e+04', '-3.4972505290949758e+05', '-1.4616788422537665e+05', '9.7252863564504100e-01', '-2.0608730663037542e-01', '-1.8606633212590740e-01']
        */
        assert!((moon_from_earth.x - -8.257_998_613_253_831e4).abs() < tol_pos);
        assert!((moon_from_earth.y - -3.497_250_529_094_976e5).abs() < tol_pos);
        assert!((moon_from_earth.z - -1.461_678_842_253_766_5e5).abs() < tol_pos);
        assert!((moon_from_earth.vx - 9.725_286_356_450_41e-1).abs() < tol_vel);
        assert!((moon_from_earth.vy - -2.060_873_066_303_754_2e-1).abs() < tol_vel);
        assert!((moon_from_earth.vz - -1.860_663_321_259_074e-1).abs() < tol_vel);

        /*
        >>> ['{:.16e}'.format(x) for x in sp.spkez(10, et, "J2000", "NONE", 399)[0]]
        ['1.0965506591533598e+08', '-9.0570891031525031e+07', '-3.9266577019474506e+07', '2.0426570124555724e+01', '2.0412112498804031e+01', '8.8484257849460111e+00']
        */
        let sun2ear_state = cosm.celestial_state(Bodies::Sun.ephem_path(), jde, eme2k, c);
        let ssb_frame = cosm.frame("SSB");
        let emb_from_ssb =
            cosm.celestial_state(Bodies::EarthBarycenter.ephem_path(), jde, ssb_frame, c);
        let sun_from_ssb = cosm.celestial_state(Bodies::Sun.ephem_path(), jde, ssb_frame, c);
        let delta_state = sun2ear_state + (-sun_from_ssb + emb_from_ssb + earth_from_emb);

        assert!(delta_state.radius().norm() < EPSILON);
        assert!(delta_state.velocity().norm() < EPSILON);

        assert!(dbg!(sun2ear_state.x - 1.096_550_659_153_359_8e8).abs() < tol_pos);
        assert!(dbg!(sun2ear_state.y - -9.057_089_103_152_503e7).abs() < tol_pos);
        assert!(dbg!(sun2ear_state.z - -3.926_657_701_947_451e7).abs() < tol_pos);
        assert!(dbg!(sun2ear_state.vx - 2.042_657_012_455_572_4e1).abs() < tol_vel);
        assert!(dbg!(sun2ear_state.vy - 2.041_211_249_880_403e1).abs() < tol_vel);
        assert!(dbg!(sun2ear_state.vz - 8.848_425_784_946_011).abs() < tol_vel);
        // And check the converse
        let sun2k = cosm.frame("Sun J2000");
        let sun2ear_state = cosm.celestial_state(&sun2k.ephem_path(), jde, eme2k, c);
        let ear2sun_state = cosm.celestial_state(&eme2k.ephem_path(), jde, sun2k, c);
        let state_sum = ear2sun_state + sun2ear_state;
        assert!(state_sum.rmag() < 1e-8);
        assert!(state_sum.vmag() < 1e-11);
    }

    #[test]
    fn test_cosm_frame_change_earth2luna() {
        let cosm = Cosm::de438();
        let eme2k = cosm.frame("EME2000");
        let luna = cosm.frame("Luna");

        let jde = Epoch::from_jde_et(2_458_823.5);
        // From JPL HORIZONS
        let lro = Orbit::cartesian(
            4.017_685_334_718_784E5,
            2.642_441_356_763_487E4,
            -3.024_209_691_251_325E4,
            -6.168_920_999_978_097E-1,
            -6.678_258_076_726_339E-1,
            4.208_264_479_358_517E-1,
            jde,
            eme2k,
        );

        let lro_jpl = Orbit::cartesian(
            -3.692_315_939_257_387E2,
            8.329_785_181_291_3E1,
            -1.764_329_108_632_533E3,
            -5.729_048_963_901_611E-1,
            -1.558_441_873_361_044,
            4.456_498_438_933_088E-2,
            jde,
            luna,
        );

        let lro_wrt_moon = cosm.frame_chg(&lro, luna);
        println!("{}", lro_jpl);
        println!("{}", lro_wrt_moon);
        let lro_moon_earth_delta = lro_jpl - lro_wrt_moon;
        // Note that the passing conditions are large. JPL uses de431MX, but nyx uses de438s.
        assert!(lro_moon_earth_delta.rmag() < 1e-2);
        assert!(lro_moon_earth_delta.vmag() < 1e-5);
        // And the converse
        let lro_wrt_earth = cosm.frame_chg(&lro_wrt_moon, eme2k);
        assert!((lro_wrt_earth - lro).rmag() < std::f64::EPSILON);
        assert!((lro_wrt_earth - lro).vmag() < std::f64::EPSILON);
    }

    #[test]
    fn test_cosm_frame_change_ven2luna() {
        let cosm = Cosm::de438();
        let luna = cosm.frame("Luna");
        let venus = cosm.frame("Venus Barycenter J2000");

        let jde = Epoch::from_jde_et(2_458_823.5);
        // From JPL HORIZONS
        let lro = Orbit::cartesian(
            -4.393_308_217_174_602E7,
            1.874_075_194_166_327E8,
            8.763_986_396_329_135E7,
            -5.054_051_490_556_286E1,
            -1.874_720_232_671_061E1,
            -6.518_342_268_306_54,
            jde,
            venus,
        );

        let lro_jpl = Orbit::cartesian(
            -3.692_315_939_257_387E2,
            8.329_785_181_291_3E1,
            -1.764_329_108_632_533E3,
            -5.729_048_963_901_611E-1,
            -1.558_441_873_361_044,
            4.456_498_438_933_088E-2,
            jde,
            luna,
        );

        let lro_wrt_moon = cosm.frame_chg(&lro, luna);
        println!("{}", lro_jpl);
        println!("{}", lro_wrt_moon);
        let lro_moon_earth_delta = lro_jpl - lro_wrt_moon;
        // Note that the passing conditions are very large. JPL uses de431MX, but nyx uses de438s.
        assert!(lro_moon_earth_delta.rmag() < 0.3);
        assert!(lro_moon_earth_delta.vmag() < 1e-5);
        // And the converse
        let lro_wrt_venus = cosm.frame_chg(&lro_wrt_moon, venus);
        assert!((lro_wrt_venus - lro).rmag() < std::f64::EPSILON);
        assert!((lro_wrt_venus - lro).vmag() < std::f64::EPSILON);
    }

    #[test]
    fn test_cosm_frame_change_ssb2luna() {
        let cosm = Cosm::de438();
        let luna = cosm.frame("Luna");
        let ssb = cosm.frame("SSB");

        let jde = Epoch::from_jde_et(2_458_823.5);
        // From JPL HORIZONS
        let lro = Orbit::cartesian(
            4.227_396_973_787_854E7,
            1.305_852_533_250_192E8,
            5.657_002_470_685_254E7,
            -2.964_638_617_895_494E1,
            7.078_704_012_700_072,
            3.779_568_779_111_446,
            jde,
            ssb,
        );

        let lro_jpl = Orbit::cartesian(
            -3.692_315_939_257_387E2,
            8.329_785_181_291_3E1,
            -1.764_329_108_632_533E3,
            -5.729_048_963_901_611E-1,
            -1.558_441_873_361_044,
            4.456_498_438_933_088E-2,
            jde,
            luna,
        );

        let lro_wrt_moon = cosm.frame_chg(&lro, luna);
        println!("{}", lro_jpl);
        println!("{}", lro_wrt_moon);
        let lro_moon_earth_delta = lro_jpl - lro_wrt_moon;
        // Note that the passing conditions are very large. JPL uses de431MX, but nyx uses de438s.
        assert!(dbg!(lro_moon_earth_delta.rmag()) < 0.3);
        assert!(dbg!(lro_moon_earth_delta.vmag()) < 1e-5);
        // And the converse
        let lro_wrt_ssb = cosm.frame_chg(&lro_wrt_moon, ssb);
        assert!((lro_wrt_ssb - lro).rmag() < std::f64::EPSILON);
        assert!((lro_wrt_ssb - lro).vmag() < std::f64::EPSILON);
    }

    #[test]
    #[ignore]
    fn test_cosm_lt_corr() {
        let cosm = Cosm::de438();

        let jde = Epoch::from_jde_et(2_452_312.500_742_881);

        let mars2k = cosm.frame("Mars Barycenter J2000");

        let out_state = cosm.celestial_state(
            Bodies::EarthBarycenter.ephem_path(),
            jde,
            mars2k,
            LTCorr::LightTime,
        );

        // Note that the following data comes from SPICE (via spiceypy).
        // There is currently a difference in computation for de438s: https://github.com/brandon-rhodes/python-jplephem/issues/33 .
        // However, in writing this test, I also checked the computed light time, which matches SPICE to 2.999058779096231e-10 seconds.
        assert!(dbg!(out_state.x - -2.577_185_470_734_315_8e8).abs() < 1e-3);
        assert!(dbg!(out_state.y - -5.814_057_247_686_307e7).abs() < 1e-3);
        assert!(dbg!(out_state.z - -2.493_960_187_215_911_6e7).abs() < 1e-3);
        assert!(dbg!(out_state.vx - -3.460_563_654_257_750_7).abs() < 1e-7);
        assert!(dbg!(out_state.vy - -3.698_207_386_702_523_5e1).abs() < 1e-7);
        assert!(dbg!(out_state.vz - -1.690_807_917_994_789_7e1).abs() < 1e-7);
    }

    #[test]
    #[ignore]
    fn test_cosm_aberration_corr() {
        let cosm = Cosm::de438();

        let jde = Epoch::from_jde_et(2_452_312.500_742_881);

        let mars2k = cosm.frame("Mars Barycenter J2000");

        let out_state = cosm.celestial_state(
            Bodies::EarthBarycenter.ephem_path(),
            jde,
            mars2k,
            LTCorr::Abberation,
        );

        assert!(dbg!(out_state.x - -2.577_231_712_700_484_4e8).abs() < 1e-3);
        assert!(dbg!(out_state.y - -5.812_356_237_533_56e7).abs() < 1e-3);
        assert!(dbg!(out_state.z - -2.493_146_410_521_204_8e7).abs() < 1e-3);
        // Reenable this test after #96 is implemented.
        dbg!(out_state.vx - -3.463_585_965_206_417);
        dbg!(out_state.vy - -3.698_169_177_803_263e1);
        dbg!(out_state.vz - -1.690_783_648_756_073e1);
    }

    #[test]
    fn test_cosm_rotation_validation() {
        let jde = Epoch::from_gregorian_utc_at_midnight(2002, 2, 7);
        let cosm = Cosm::de438();

        println!("Available ephems: {:?}", cosm.xb.ephemeris_get_names());
        println!("Available frames: {:?}", cosm.frames_get_names());

        let sun2k = cosm.frame("Sun J2000");
        let sun_iau = cosm.frame("IAU Sun");
        let ear_sun_2k = cosm.celestial_state(Bodies::Earth.ephem_path(), jde, sun2k, LTCorr::None);
        let ear_sun_iau = cosm.frame_chg(&ear_sun_2k, sun_iau);
        let ear_sun_2k_prime = cosm.frame_chg(&ear_sun_iau, sun2k);

        assert!(
            (ear_sun_2k.rmag() - ear_sun_iau.rmag()).abs() <= 1e-6,
            "a single rotation changes rmag"
        );
        assert!(
            (ear_sun_2k_prime - ear_sun_2k).rmag() <= 1e-6,
            "reverse rotation does not match initial state"
        );

        // Test an EME2k to Earth IAU rotation

        let eme2k = cosm.frame("EME2000");
        let earth_iau = cosm.frame("IAU Earth"); // 2000 Model!!
        println!("{:?}\n{:?}", eme2k, earth_iau);
        let dt = Epoch::from_gregorian_tai_at_noon(2000, 1, 1);

        let state_eme2k = Orbit::cartesian(
            5_946.673_548_288_958,
            1_656.154_606_023_661,
            2_259.012_129_598_249,
            -3.098_683_050_943_824,
            4.579_534_132_135_011,
            6.246_541_551_539_432,
            dt,
            eme2k,
        );
        let state_ecef = cosm.frame_chg(&state_eme2k, earth_iau);
        println!("{}\n{}", state_eme2k, state_ecef);
        let delta_state = cosm.frame_chg(&state_ecef, eme2k) - state_eme2k;
        assert!(
            delta_state.rmag().abs() < 1e-9,
            "Inverse rotation is broken"
        );
        assert!(
            delta_state.vmag().abs() < 1e-9,
            "Inverse rotation is broken"
        );
        // Monte validation
        // EME2000 state:
        // State (km, km/sec)
        // 'Earth' -> 'test' in 'EME2000' at '01-JAN-2000 12:00:00.0000 TAI'
        // Pos:  5.946673548288958e+03  1.656154606023661e+03  2.259012129598249e+03
        // Vel: -3.098683050943824e+00  4.579534132135011e+00  6.246541551539432e+00Earth Body Fixed state:
        // State (km, km/sec)
        // 'Earth' -> 'test' in 'Earth Body Fixed' at '01-JAN-2000 12:00:00.0000 TAI'
        // Pos: -5.681756320398799e+02  6.146783778323857e+03  2.259012130187828e+03
        // Vel: -4.610834400780483e+00 -2.190121576903486e+00  6.246541569551255e+00
        assert!(dbg!(state_ecef.x - -5.681_756_320_398_799e2).abs() < 1e-5);
        assert!(dbg!(state_ecef.y - 6.146_783_778_323_857e3).abs() < 1e-5);
        assert!(dbg!(state_ecef.z - 2.259_012_130_187_828e3).abs() < 1e-5);
        // TODO: Fix the velocity computation

        // Case 2
        // Earth Body Fixed state:
        // State (km, km/sec)
        // 'Earth' -> 'test' in 'Earth Body Fixed' at '31-JAN-2000 12:00:00.0000 TAI'
        // Pos:  3.092802381110541e+02 -3.431791232988777e+03  6.891017545171710e+03
        // Vel:  6.917077556761001e+00  6.234631407415389e-01  4.062487128428244e-05
        let state_eme2k = Orbit::cartesian(
            -2436.45,
            -2436.45,
            6891.037,
            5.088_611,
            -5.088_611,
            0.0,
            Epoch::from_gregorian_tai_at_noon(2000, 1, 31),
            eme2k,
        );

        let state_ecef = cosm.frame_chg(&state_eme2k, earth_iau);
        println!("{}\n{}", state_eme2k, state_ecef);
        assert!(dbg!(state_ecef.x - 309.280_238_111_054_1).abs() < 1e-1);
        assert!(dbg!(state_ecef.y - -3_431.791_232_988_777).abs() < 1e-1);
        assert!(dbg!(state_ecef.z - 6_891.017_545_171_71).abs() < 1e-1);

        // Case 3
        // Earth Body Fixed state:
        // State (km, km/sec)
        // 'Earth' -> 'test' in 'Earth Body Fixed' at '01-MAR-2000 12:00:00.0000 TAI'
        // Pos: -1.424497118292030e+03 -3.137502417055381e+03  6.890998090503171e+03
        // Vel:  6.323912379829687e+00 -2.871020900962905e+00  8.125749038014632e-05
        let state_eme2k = Orbit::cartesian(
            -2436.45,
            -2436.45,
            6891.037,
            5.088_611,
            -5.088_611,
            0.0,
            Epoch::from_gregorian_tai_at_noon(2000, 3, 1),
            eme2k,
        );

        let state_ecef = cosm.frame_chg(&state_eme2k, earth_iau);
        println!("{}\n{}", state_eme2k, state_ecef);
        assert!(dbg!(state_ecef.x - -1_424.497_118_292_03).abs() < 1e0);
        assert!(dbg!(state_ecef.y - -3_137.502_417_055_381).abs() < 1e-1);
        assert!(dbg!(state_ecef.z - 6_890.998_090_503_171).abs() < 1e-1);

        // Ground station example
        let dt = Epoch::from_gregorian_tai_hms(2020, 1, 1, 0, 0, 20);
        let gs = Orbit::cartesian(
            -4461.153491497329,
            2682.445251105359,
            -3674.3793821716713,
            -0.19560699645796042,
            -0.32531244947129817,
            0.0,
            dt,
            earth_iau,
        );
        let gs_eme = cosm.frame_chg(&gs, eme2k);
        println!("{}\n{}", gs, gs_eme);
    }

    #[test]
    fn test_cosm_fix_frame_name() {
        assert_eq!(
            Cosm::fix_frame_name("Mars barycenter j2000"),
            "Mars Barycenter J2000"
        );
    }
}
