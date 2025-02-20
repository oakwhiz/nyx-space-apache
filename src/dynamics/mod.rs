extern crate hyperdual;

use self::hyperdual::{hyperspace_from_vector, Hyperdual, Owned};
use crate::celestia::{Orbit, SpacecraftState};
use crate::dimensions::allocator::Allocator;
use crate::dimensions::{DefaultAllocator, DimName, Matrix3, MatrixN, Vector3, VectorN, U7};
use crate::State;

pub use crate::errors::NyxError;

/// The orbital module handles all Cartesian based orbital dynamics.
///
/// It is up to the engineer to ensure that the coordinate frames of the different dynamics borrowed
/// from this module match, or perform the appropriate coordinate transformations.
pub mod orbital;
pub use self::orbital::*;

/// The gravity module handles spherical harmonics only. It _must_ be combined with a OrbitalDynamics dynamics
///
/// This module allows loading gravity models from [PDS](http://pds-geosciences.wustl.edu/), [EGM2008](http://earth-info.nga.mil/GandG/wgs84/gravitymod/egm2008/) and GMAT's own COF files.
// pub mod gravity;

/// The drag module handles drag in a very basic fashion. Do not use for high fidelity dynamics.
// pub mod drag;

/// The spacecraft module allows for simulation of spacecraft dynamics in general, including propulsion/maneuvers.
pub mod spacecraft;
pub use self::spacecraft::*;

/// Defines a few examples of thrust controllers.
pub mod thrustctrl;

/// Defines some velocity change controllers.
pub mod deltavctrl;

/// Defines solar radiation pressure models
pub mod solarpressure;
pub use self::solarpressure::*;

/// Define drag models
pub mod drag;
pub use self::drag::*;

/// Define the spherical harmonic models.
pub mod sph_harmonics;
pub use self::sph_harmonics::*;

/// The `Dynamics` trait handles and stores any equation of motion *and* the state is integrated.
///
/// Its design is such that several of the provided dynamics can be combined fairly easily. However,
/// when combining the dynamics (e.g. integrating both the attitude of a spaceraft and its orbital
///  parameters), it is up to the implementor to handle time and state organization correctly.
/// For time management, I highly recommend using `hifitime` which is thoroughly validated.
pub trait Dynamics: Clone
where
    DefaultAllocator: Allocator<f64, <Self::StateType as State>::Size>,
{
    /// Defines the state size for these dynamics.
    // type StateSize: DimName;
    /// Defines the size of the vector to be propagated, equalto StateSize if no STM will ever be computed.
    // type PropVecSize: DimName;
    /// The state of the associated hyperdual state, almost always StateType + U1
    type HyperdualSize: DimName;
    // type StateType: State<Self::StateSize, PropVecSize = Self::PropVecSize>;
    type StateType: State;

    /// Defines the equations of motion for these dynamics, or a combination of provided dynamics.
    /// The time delta_t is in **seconds** PAST the context epoch. The state vector is the state which
    /// changes for every intermediate step of the integration. The state context is the state of
    /// what is being propagated, it should allow rebuilding a new state context from the
    /// provided state vector.
    fn eom(
        &self,
        delta_t: f64,
        state_vec: &VectorN<f64, <Self::StateType as State>::PropVecSize>,
        state_ctx: &Self::StateType,
    ) -> Result<VectorN<f64, <Self::StateType as State>::PropVecSize>, NyxError>
    where
        DefaultAllocator: Allocator<f64, <Self::StateType as State>::PropVecSize>;

    /// Defines the equations of motion for Dual numbers for these dynamics.
    /// _All_ dynamics need to allow for automatic differentiation. However, if differentiation is not supported,
    /// then the dynamics should prevent initialization with a context which has an STM defined.
    fn dual_eom(
        &self,
        delta_t: f64,
        state_vec: &VectorN<Hyperdual<f64, Self::HyperdualSize>, <Self::StateType as State>::Size>,
        state_ctx: &Self::StateType,
    ) -> Result<
        (
            VectorN<f64, <Self::StateType as State>::Size>,
            MatrixN<f64, <Self::StateType as State>::Size>,
        ),
        NyxError,
    >
    where
        DefaultAllocator: Allocator<f64, Self::HyperdualSize>
            + Allocator<f64, <Self::StateType as State>::Size>
            + Allocator<f64, <Self::StateType as State>::Size, <Self::StateType as State>::Size>
            + Allocator<Hyperdual<f64, Self::HyperdualSize>, <Self::StateType as State>::Size>,
        Owned<f64, Self::HyperdualSize>: Copy;

    /// Computes both the state and the gradient of the dynamics. This function is pre-implemented.
    fn eom_grad(
        &self,
        delta_t_s: f64,
        state_vec: &VectorN<f64, <Self::StateType as State>::Size>,
        state_ctx: &Self::StateType,
    ) -> Result<
        (
            VectorN<f64, <Self::StateType as State>::Size>,
            MatrixN<f64, <Self::StateType as State>::Size>,
        ),
        NyxError,
    >
    where
        DefaultAllocator: Allocator<f64, <Self::StateType as State>::Size>
            + Allocator<f64, <Self::StateType as State>::Size, <Self::StateType as State>::Size>
            + Allocator<f64, Self::HyperdualSize>
            + Allocator<Hyperdual<f64, Self::HyperdualSize>, <Self::StateType as State>::Size>,
        Owned<f64, Self::HyperdualSize>: Copy,
    {
        let hyperstate: VectorN<
            Hyperdual<f64, Self::HyperdualSize>,
            <Self::StateType as State>::Size,
        > = hyperspace_from_vector(&state_vec);

        let (state, grad) = self.dual_eom(delta_t_s, &hyperstate, &state_ctx)?;

        Ok((state, grad))
    }

    /// Optionally performs some final changes after each successful integration of the equations of motion.
    /// For example, this can be used to update the GNC mode.
    fn finally(&self, next_state: Self::StateType) -> Result<Self::StateType, NyxError> {
        Ok(next_state)
    }
}

/// The `ForceModel` trait handles immutable dynamics which return a force. Those will be divided by the mass of the spacecraft to compute the acceleration (F = ma).
///
/// Examples include Solar Radiation Pressure, drag, etc., i.e. forces which do not need to save the current state, only act on it.
pub trait ForceModel: Send + Sync {
    /// Defines the equations of motion for this force model from the provided osculating state.
    fn eom(&self, ctx: &SpacecraftState) -> Result<Vector3<f64>, NyxError>;

    /// Force models must implement their partials, although those will only be called if the propagation requires the
    /// computation of the STM. The `osc_ctx` is the osculating context, i.e. it changes for each sub-step of the integrator.
    fn dual_eom(
        &self,
        radius: &Vector3<Hyperdual<f64, U7>>,
        osc_ctx: &SpacecraftState,
    ) -> Result<(Vector3<f64>, Matrix3<f64>), NyxError>;
}

/// The `AccelModel` trait handles immutable dynamics which return an acceleration. Those can be added directly to Celestial Dynamics for example.
///
/// Examples include spherical harmonics, i.e. accelerations which do not need to save the current state, only act on it.
pub trait AccelModel: Send + Sync {
    /// Defines the equations of motion for this force model from the provided osculating state in the integration frame.
    fn eom(&self, osc: &Orbit) -> Result<Vector3<f64>, NyxError>;

    /// Acceleration models must implement their partials, although those will only be called if the propagation requires the
    /// computation of the STM.
    fn dual_eom(
        &self,
        radius: &Vector3<Hyperdual<f64, U7>>,
        osc_ctx: &Orbit,
    ) -> Result<(Vector3<f64>, Matrix3<f64>), NyxError>;
}
