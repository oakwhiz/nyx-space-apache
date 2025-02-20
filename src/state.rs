use crate::celestia::{Frame, GuidanceMode, Orbit, SpacecraftState};
use crate::dimensions::allocator::Allocator;
use crate::dimensions::{
    DefaultAllocator, DimName, Matrix6, MatrixN, Vector1, VectorN, U42, U43, U6, U7,
};
use crate::errors::NyxError;
use crate::time::{Duration, Epoch};
use std::fmt;
use std::ops::Add;

/// A trait allowing for something to have an epoch
pub trait TimeTagged {
    /// Retrieve the Epoch
    fn epoch(&self) -> Epoch;
    /// Set the Epoch
    fn set_epoch(&mut self, epoch: Epoch);

    /// Shift this epoch by a duration (can be negative)
    fn shift_by(&mut self, duration: Duration) {
        self.set_epoch(self.epoch() + duration);
    }
}

/// A trait for generate propagation and estimation state.
/// The first parameter is the size of the state, the second is the size of the propagated state including STM and extra items.
pub trait State:
    TimeTagged + Copy + Clone + PartialEq + fmt::Display + fmt::LowerExp + Send + Sync
where
    Self: Sized,
    DefaultAllocator: Allocator<f64, Self::Size>,
{
    /// Size of the state and its STM
    type Size: DimName;
    type PropVecSize: DimName;
    /// Initialize an empty state
    fn zeros() -> Self;

    /// Return this state as a vector for the propagation/estimation
    fn as_vector(&self) -> Result<VectorN<f64, Self::PropVecSize>, NyxError>
    where
        DefaultAllocator: Allocator<f64, Self::PropVecSize>;

    /// Return this state as a vector for the propagation/estimation
    fn stm(&self) -> Result<MatrixN<f64, Self::Size>, NyxError>
    where
        DefaultAllocator: Allocator<f64, Self::Size, Self::Size>;

    /// Set this state
    fn set(
        &mut self,
        epoch: Epoch,
        vector: &VectorN<f64, Self::PropVecSize>,
    ) -> Result<(), NyxError>
    where
        DefaultAllocator: Allocator<f64, Self::PropVecSize>;

    /// Reconstruct a new State from the provided delta time in seconds compared to the current state
    /// and with the provided vector.
    fn ctor_from(self, delta_t_s: f64, vector: &VectorN<f64, Self::PropVecSize>) -> Self
    where
        DefaultAllocator: Allocator<f64, Self::PropVecSize>,
    {
        let mut me = self;
        me.set(me.epoch() + delta_t_s, vector).unwrap();
        me
    }

    fn add(self, other: VectorN<f64, Self::Size>) -> Self;
}

/// Implementation of Orbit as a State for orbital dynamics with STM
impl State for Orbit {
    type Size = U6;
    type PropVecSize = U42;
    /// Returns a state whose position, velocity and frame are zero, and STM is I_{6x6}.
    fn zeros() -> Self {
        let frame = Frame::Celestial {
            axb_id: 0,
            exb_id: 0,
            gm: 159.0,
            parent_axb_id: None,
            parent_exb_id: None,
            ephem_path: [None, None, None],
            frame_path: [None, None, None],
        };

        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            dt: Epoch::from_tai_seconds(0.0),
            frame,
            stm: Some(Matrix6::identity()),
        }
    }

    fn as_vector(&self) -> Result<VectorN<f64, U42>, NyxError> {
        let mut as_vec = VectorN::<f64, U42>::zeros();
        as_vec[0] = self.x;
        as_vec[1] = self.y;
        as_vec[2] = self.z;
        as_vec[3] = self.vx;
        as_vec[4] = self.vy;
        as_vec[5] = self.vz;
        let mut stm_idx = 6;
        if let Some(stm) = self.stm {
            for i in 0..6 {
                for j in 0..6 {
                    as_vec[stm_idx] = stm[(i, j)];
                    stm_idx += 1;
                }
            }
        }
        Ok(as_vec)
    }

    fn set(&mut self, epoch: Epoch, vector: &VectorN<f64, U42>) -> Result<(), NyxError> {
        self.set_epoch(epoch);
        self.x = vector[0];
        self.y = vector[1];
        self.z = vector[2];
        self.vx = vector[3];
        self.vy = vector[4];
        self.vz = vector[5];
        // And update the STM if applicable
        if let Some(mut stm_prev) = self.stm {
            let mut stm_k_to_0 = Matrix6::zeros();
            let mut stm_idx = 6;
            for i in 0..6 {
                for j in 0..6 {
                    stm_k_to_0[(i, j)] = vector[(stm_idx, 0)];
                    stm_idx += 1;
                }
            }

            // let mut stm_prev = self.state.stm();
            if !stm_prev.try_inverse_mut() {
                error!("STM not invertible: {}", stm_prev);
                return Err(NyxError::SingularStateTransitionMatrix);
            }
            self.stm = Some(stm_k_to_0 * stm_prev);
            // self.state.stm = Some(stm_k_to_0);
        }
        Ok(())
    }

    fn stm(&self) -> Result<Matrix6<f64>, NyxError> {
        match self.stm {
            Some(stm) => Ok(stm),
            None => Err(NyxError::StateTransitionMatrixUnset),
        }
    }

    fn add(self, other: VectorN<f64, Self::Size>) -> Self {
        self + other
    }
}

impl Add<VectorN<f64, U6>> for Orbit {
    type Output = Self;

    /// Adds the provided state deviation to this orbit
    fn add(self, other: VectorN<f64, U6>) -> Self {
        let mut me = self;
        me.x += other[0];
        me.y += other[1];
        me.z += other[2];
        me.vx += other[3];
        me.vy += other[4];
        me.vz += other[5];

        me
    }
}

impl TimeTagged for SpacecraftState {
    fn epoch(&self) -> Epoch {
        self.orbit.dt
    }

    fn set_epoch(&mut self, epoch: Epoch) {
        self.orbit.dt = epoch
    }
}

impl State for SpacecraftState {
    type Size = U7;
    type PropVecSize = U43;
    fn zeros() -> Self {
        Self {
            orbit: Orbit::zeros(),
            dry_mass_kg: 0.0,
            fuel_mass_kg: 0.0,
            thruster: None,
            mode: GuidanceMode::Coast,
        }
    }

    fn as_vector(&self) -> Result<VectorN<f64, U43>, NyxError> {
        let orb_vec: VectorN<f64, U42> = self.orbit.as_vector()?;
        Ok(VectorN::<f64, U43>::from_iterator(
            orb_vec
                .iter()
                .chain(Vector1::new(self.fuel_mass_kg).iter())
                .cloned(),
        ))
    }

    fn set(&mut self, epoch: Epoch, vector: &VectorN<f64, U43>) -> Result<(), NyxError> {
        self.set_epoch(epoch);
        let orbit_vec = vector.fixed_rows::<U42>(0).into_owned();
        self.orbit.set(epoch, &orbit_vec)?;
        self.fuel_mass_kg = vector[U43::dim() - 1];
        Ok(())
    }

    /// WARNING: Currently the STM assumes that the fuel mass is constant at ALL TIMES!
    fn stm(&self) -> Result<MatrixN<f64, U7>, NyxError> {
        match self.orbit.stm {
            Some(stm) => {
                let mut rtn = MatrixN::<f64, U7>::zeros();
                for i in 0..6 {
                    for j in 0..6 {
                        rtn[(i, j)] = stm[(i, j)];
                    }
                }
                rtn[(6, 6)] = 0.0;
                Ok(rtn)
            }
            None => Err(NyxError::StateTransitionMatrixUnset),
        }
    }

    fn add(self, other: VectorN<f64, Self::Size>) -> Self {
        self + other
    }
}

impl Add<VectorN<f64, U7>> for SpacecraftState {
    type Output = Self;

    /// Adds the provided state deviation to this orbit
    fn add(self, other: VectorN<f64, U7>) -> Self {
        let mut me = self;
        me.orbit.x += other[0];
        me.orbit.y += other[1];
        me.orbit.z += other[2];
        me.orbit.vx += other[3];
        me.orbit.vy += other[4];
        me.orbit.vz += other[5];
        me.fuel_mass_kg += other[6];

        me
    }
}

impl Add<VectorN<f64, U6>> for SpacecraftState {
    type Output = Self;

    /// Adds the provided state deviation to this orbit
    fn add(self, other: VectorN<f64, U6>) -> Self {
        let mut me = self;
        me.orbit.x += other[0];
        me.orbit.y += other[1];
        me.orbit.z += other[2];
        me.orbit.vx += other[3];
        me.orbit.vy += other[4];
        me.orbit.vz += other[5];

        me
    }
}

#[test]
fn test_set_state() {
    let delta_t_s: f64 = 0.0;
    let mut new_vec = vec![0.0; 42];
    new_vec[0] = 1.0;
    new_vec[1] = 2.0;
    new_vec[2] = 3.0;
    new_vec[3] = 4.0;
    new_vec[4] = 5.0;
    new_vec[5] = 6.0;

    let state = VectorN::<f64, U42>::from_iterator(new_vec);
    let dummy_frame = Frame::Celestial {
        axb_id: 0,
        exb_id: 399,
        gm: 398600.4415,
        parent_axb_id: None,
        parent_exb_id: None,
        ephem_path: [None, None, None],
        frame_path: [None, None, None],
    };
    let ctx = Orbit::cartesian(
        6678.1363,
        2.0,
        3.0,
        4.0,
        7.725760634075587,
        6.0,
        Epoch::from_tai_days(0.0),
        dummy_frame,
    );
    let osc = ctx.ctor_from(delta_t_s, &state);
    println!("{}", osc);
}
