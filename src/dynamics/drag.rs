use super::hyperdual::Hyperdual;
use super::ForceModel;
use crate::celestia::{Cosm, Frame, SpacecraftState};
use crate::dimensions::{Matrix3, Vector3, U7};
use crate::errors::NyxError;
use std::sync::Arc;

/// Density in kg/m^3 and altitudes in meters, not kilometers!
#[derive(Clone, Copy, Debug)]
pub enum AtmDensity {
    Constant(f64),
    Exponential { rho0: f64, r0: f64, ref_alt_m: f64 },
    StdAtm { max_alt_m: f64 },
}

/// `ConstantDrag` implements a constant drag model as defined in Vallado, 4th ed., page 551, with an important caveat.
///
/// **WARNING:** This basic model assumes that the velocity of the spacecraft is identical to the velocity of the upper atmosphere,
/// This is a **bad** assumption and **should not** be used for high fidelity simulations.
/// This will be resolved after https://gitlab.com/chrisrabotin/nyx/issues/93 is implemented.
#[derive(Clone)]
pub struct ConstantDrag {
    /// in m^2
    pub sc_area: f64,
    /// coefficient of drag; (spheres are between 2.0 and 2.1, use 2.2 in Earth's atmosphere).
    pub cd: f64,
    /// atmospheric density in kg/m^3
    pub rho: f64,
    /// Geoid causing the drag
    pub drag_frame: Frame,
    /// a Cosm reference is needed to convert to the state around the correct planet
    pub cosm: Arc<Cosm>,
}

impl ForceModel for ConstantDrag {
    fn eom(&self, ctx: &SpacecraftState) -> Result<Vector3<f64>, NyxError> {
        let osc = self.cosm.frame_chg(&ctx.orbit, self.drag_frame);
        let velocity = osc.velocity();
        Ok(-0.5 * self.rho * self.cd * self.sc_area * velocity.norm() * velocity)
    }

    fn dual_eom(
        &self,
        _radius: &Vector3<Hyperdual<f64, U7>>,
        _osc_ctx: &SpacecraftState,
    ) -> Result<(Vector3<f64>, Matrix3<f64>), NyxError> {
        Err(NyxError::PartialsUndefined)
    }
}

/// `Drag` implements all three drag models.
#[derive(Clone)]
pub struct Drag {
    /// Density computation method
    pub density: AtmDensity,
    /// in m^2
    pub sc_area: f64,
    /// coefficient of drag; (spheres are between 2.0 and 2.1, use 2.2 in Earth's atmosphere).
    pub cd: f64,
    /// Frame to compute the drag in
    pub drag_frame: Frame,
    /// a Cosm reference is needed to convert to the state around the correct planet
    pub cosm: Arc<Cosm>,
}

impl Drag {
    /// Common exponential drag model for the Earth
    pub fn earth_exp(sc_area: f64, cd: f64, cosm: Arc<Cosm>) -> Arc<Self> {
        Arc::new(Self {
            density: AtmDensity::Exponential {
                rho0: 3.614e-13,
                r0: 700_000.0,
                ref_alt_m: 88_667.0,
            },
            sc_area,
            cd,
            drag_frame: cosm.frame("IAU Earth"),
            cosm,
        })
    }

    /// Drag model which uses the standard atmosphere 1976 model for atmospheric density
    pub fn std_atm1976(sc_area: f64, cd: f64, cosm: Arc<Cosm>) -> Arc<Self> {
        Arc::new(Self {
            density: AtmDensity::StdAtm {
                max_alt_m: 1_000_000.0,
            },
            sc_area,
            cd,
            drag_frame: cosm.frame("IAU Earth"),
            cosm,
        })
    }
}

impl ForceModel for Drag {
    fn eom(&self, ctx: &SpacecraftState) -> Result<Vector3<f64>, NyxError> {
        let osc = self.cosm.frame_chg(&ctx.orbit, self.drag_frame);
        match self.density {
            AtmDensity::Constant(rho) => {
                let velocity = osc.velocity();
                Ok(-0.5 * rho * self.cd * self.sc_area * velocity.norm() * velocity)
            }
            AtmDensity::Exponential {
                rho0,
                r0,
                ref_alt_m,
            } => {
                let rho = rho0
                    * (-(osc.rmag() - (r0 + self.drag_frame.equatorial_radius())) / ref_alt_m)
                        .exp();

                let velocity_eme2k = self
                    .cosm
                    .frame_chg(&osc, self.cosm.frame("EME2000"))
                    .velocity();

                let velocity = velocity_eme2k - osc.velocity();
                Ok(-0.5 * rho * self.cd * self.sc_area * velocity.norm() * velocity)
            }
            AtmDensity::StdAtm { max_alt_m } => {
                let altitude_km = osc.rmag() - self.drag_frame.equatorial_radius();
                let rho = if altitude_km > max_alt_m / 1_000.0 {
                    // Use a constant density
                    10.0_f64.powf((-7e-5) * altitude_km - 14.464)
                } else {
                    // Code from AVS/Schaub's Basilisk
                    // Calculating the density based on a scaled 6th order polynomial fit to the log of density
                    let scale = (altitude_km - 526.8000) / 292.8563;
                    let logdensity =
                        0.34047 * scale.powi(6) - 0.5889 * scale.powi(5) - 0.5269 * scale.powi(4)
                            + 1.0036 * scale.powi(3)
                            + 0.60713 * scale.powi(2)
                            - 2.3024 * scale
                            - 12.575;

                    /* Calculating density by raising 10 to the log of density */
                    10.0_f64.powf(logdensity)
                };

                let velocity_eme2k = self
                    .cosm
                    .frame_chg(&osc, self.cosm.frame("EME2000"))
                    .velocity();

                let velocity = velocity_eme2k - osc.velocity();
                Ok(-0.5 * rho * self.cd * self.sc_area * velocity.norm() * velocity)
            }
        }
    }

    fn dual_eom(
        &self,
        _radius: &Vector3<Hyperdual<f64, U7>>,
        _osc_ctx: &SpacecraftState,
    ) -> Result<(Vector3<f64>, Matrix3<f64>), NyxError> {
        Err(NyxError::PartialsUndefined)
    }
}
