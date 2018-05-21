extern crate hifitime;
extern crate nalgebra as na;

use self::na::{DefaultAllocator, Dim, DimName, VectorN};
use self::na::allocator::Allocator;

/// The celestial module handles all Cartesian based dynamics.
///
/// It is up to the engineer to ensure that the coordinate frames of the different dynamics borrowed
/// from this module match, or perform the appropriate coordinate transformations.
pub mod celestial;

/// The gravity module handles spherical harmonics only. It _must_ be combined with a TwoBody dynamics
///
/// This module allows loading gravity models from [PDS](http://pds-geosciences.wustl.edu/), [EGM2008](http://earth-info.nga.mil/GandG/wgs84/gravitymod/egm2008/) and GMAT's own COF files.
pub mod gravity;

/// The drag module handles drag in a very basic fashion. Do not use for high fidelity dynamics.
pub mod drag;

/// The angular momentum module handles all angular momentum dynamics.
///
/// Note that this module does not handle attitude parameters or control. Refer to the relevant modules.
pub mod momentum;

/// The `Dynamics` trait handles and stores any equation of motion *and* the state is integrated.
///
/// Its design is such that several of the provided dynamics can be combined fairly easily. However,
/// when combining the dynamics (e.g. integrating both the attitude of a spaceraft and its orbital
///  parameters), it is up to the implementor to handle time and state organization correctly.
/// For time management, I highly recommend using `hifitime` which is thoroughly validated.
pub trait Dynamics
where
    Self: Sized,
{
    /// Defines the state size for these dynamics. It must be imported from `nalgebra`.
    type StateSize: Dim + DimName;
    /// Returns the time of the current state
    fn time(&self) -> f64;

    /// Returns the current state of the dynamics so it can be integrated.
    fn state(&self) -> VectorN<f64, Self::StateSize>
    where
        DefaultAllocator: Allocator<f64, Self::StateSize>;

    /// Defines the equations of motion for these dynamics, or a combination of provided dynamics. XXX: Is this going to work in `derive`?!
    fn eom(&self, t: f64, state: &VectorN<f64, Self::StateSize>) -> VectorN<f64, Self::StateSize>
    where
        DefaultAllocator: Allocator<f64, Self::StateSize>;

    /// Updates the internal state of the dynamics.
    ///
    /// NOTE: I do not think that this should be a Box<Self> because my understanding is that it
    /// would invalidate the previous state entirely. That would also mean that `fn state` would
    /// return a Box<VectorN> which would then transfer the ownership to whoever calls it. I do not
    /// think this is a good idea in this case. In fact, we can assume that when integrating the
    /// attitude of two instruments, we might need to "read" the attitude of the spacecraft, "read"
    /// that of an instrument, and compute the DCM between both. If the first attitude lost ownership
    /// of its state, it wouldn't be able to integrate it anymore.
    fn set_state(&mut self, new_t: f64, new_state: &VectorN<f64, Self::StateSize>)
    where
        DefaultAllocator: Allocator<f64, Self::StateSize>;
}
