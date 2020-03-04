extern crate hyperdual;
extern crate nalgebra as na;
extern crate serde;

use self::hyperdual::{hyperspace_from_vector, Hyperdual, Owned};
use self::na::allocator::Allocator;
use self::na::{DefaultAllocator, DimName, MatrixMN, VectorN};
use crate::hifitime::Epoch;
use celestia::{Frame, State};
use dynamics::Dynamics;
use std::fmt;

/// Provides the Kalman filters. The [examples](https://github.com/ChristopherRabotin/nyx/tree/master/examples) folder may help in the setup.
pub mod kalman;

/// Provides a range and range rate measuring models.
pub mod ranging;

/// Provides Estimate handling functionalities.
pub mod estimate;

/// Provide Residual handling functionalities.
pub mod residual;

/// Provides some helper for filtering.
pub mod ui;

/// Provides the Square Root Information Filter
pub mod srif;

/// A trait container to specify that given dynamics support linearization, and can be used for state transition matrix computation.
///
/// This trait will likely be made obsolete after the implementation of [#32](https://github.com/ChristopherRabotin/nyx/issues/32).
pub trait Estimable<N>
where
    Self: Dynamics + Sized,
{
    /// Defines the state size of the estimated state
    type LinStateSize: DimName;
    /// Returns the estimated state
    fn extract_estimated_state(
        &self,
        prop_state: &Self::StateType,
    ) -> VectorN<f64, Self::LinStateSize>
    where
        DefaultAllocator: Allocator<f64, Self::LinStateSize>;

    /// Returns the estimated state
    fn estimated_state(&self) -> VectorN<f64, Self::LinStateSize>
    where
        DefaultAllocator: Allocator<f64, Self::LinStateSize>,
    {
        self.extract_estimated_state(&self.state())
    }

    /// Sets the estimated state
    fn set_estimated_state(&mut self, new_state: VectorN<f64, Self::LinStateSize>)
    where
        DefaultAllocator: Allocator<f64, Self::LinStateSize>;

    /// Defines the gradient of the equations of motion for these dynamics.
    fn stm(&self) -> MatrixMN<f64, Self::LinStateSize, Self::LinStateSize>
    where
        DefaultAllocator: Allocator<f64, Self::LinStateSize>
            + Allocator<f64, Self::LinStateSize, Self::LinStateSize>,
    {
        self.extract_stm(&self.state())
    }

    /// Converts the Dynamics' state type to a measurement to be ingested in a filter
    fn to_measurement(&self, prop_state: &Self::StateType) -> (Epoch, N);

    /// Extracts the STM from the dynamics state
    fn extract_stm(
        &self,
        prop_state: &Self::StateType,
    ) -> MatrixMN<f64, Self::LinStateSize, Self::LinStateSize>
    where
        DefaultAllocator: Allocator<f64, Self::LinStateSize>
            + Allocator<f64, Self::LinStateSize, Self::LinStateSize>;
}

/// Defines a Filter trait where S is the size of the estimated state, A the number of acceleration components of the EOMs (used for process noise matrix size), M the size of the measurements.
pub trait Filter<S, A, M>
where
    S: DimName,
    A: DimName,
    M: DimName,
    DefaultAllocator: Allocator<f64, M>
        + Allocator<f64, S>
        + Allocator<f64, M, M>
        + Allocator<f64, M, S>
        + Allocator<f64, S, S>
        + Allocator<f64, A, A>
        + Allocator<f64, S, A>
        + Allocator<f64, A, S>,
{
    type Estimate: estimate::Estimate<S>;

    /// Returns the previous estimate
    fn previous_estimate(&self) -> &Self::Estimate;

    /// Update the State Transition Matrix (STM). This function **must** be called in between each
    /// call to `time_update` or `measurement_update`.
    fn update_stm(&mut self, new_stm: MatrixMN<f64, S, S>);

    /// Update the sensitivity matrix (or "H tilde"). This function **must** be called prior to each
    /// call to `measurement_update`.
    fn update_h_tilde(&mut self, h_tilde: MatrixMN<f64, M, S>);

    /// Computes a time update/prediction (i.e. advances the filter estimate with the updated STM).
    ///
    /// Returns a FilterError if the STM was not updated.
    fn time_update(&mut self, dt: Epoch) -> Result<Self::Estimate, FilterError>;

    /// Computes the measurement update with a provided real observation and computed observation.
    ///
    ///Returns a FilterError if the STM or sensitivity matrices were not updated.
    fn measurement_update(
        &mut self,
        dt: Epoch,
        real_obs: VectorN<f64, M>,
        computed_obs: VectorN<f64, M>,
    ) -> Result<(Self::Estimate, residual::Residual<M>), FilterError>;

    /// Returns whether the filter is an extended filter (e.g. EKF)
    fn is_extended(&self) -> bool;

    /// Sets the filter to be extended or not depending on the value of status
    fn set_extended(&mut self, status: bool);

    /// Sets the process noise matrix in the frame of the estimated state
    fn set_process_noise(&mut self, prc: MatrixMN<f64, A, A>);
}

/// Stores the different kinds of filter errors.
#[derive(Debug, PartialEq)]
pub enum FilterError {
    StateTransitionMatrixNotUpdated,
    SensitivityNotUpdated,
    GainSingular,
    StateTransitionMatrixSingular,
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            FilterError::StateTransitionMatrixNotUpdated => {
                write!(f, "STM was not updated prior to time or measurement update")
            }
            FilterError::SensitivityNotUpdated => write!(
                f,
                "The measurement matrix H_tilde was not updated prior to measurement update"
            ),
            FilterError::GainSingular => write!(
                f,
                "Gain could not be computed because H*P_bar*H + R is singular"
            ),
            FilterError::StateTransitionMatrixSingular => {
                write!(f, "STM is singular, smoothing cannot proceed")
            }
        }
    }
}

/// A trait defining a measurement of size `MeasurementSize`
pub trait Measurement
where
    Self: Sized,
    DefaultAllocator: Allocator<f64, Self::MeasurementSize>
        + Allocator<f64, Self::MeasurementSize, Self::StateSize>,
{
    /// Defines the state size of the estimated state
    type StateSize: DimName;
    /// Defines how much data is measured. For example, if measuring range and range rate, this should be of size 2 (nalgebra::U2).
    type MeasurementSize: DimName;

    /// Computes a new measurement from the provided information.
    fn new<F: Frame>(dt: Epoch, tx: State<F>, rx: State<F>, visible: bool) -> Self;

    /// Returns the measurement/observation as a vector.
    fn observation(&self) -> VectorN<f64, Self::MeasurementSize>
    where
        DefaultAllocator: Allocator<f64, Self::MeasurementSize>;

    /// Returns the measurement sensitivity (often referred to as H tilde).
    fn sensitivity(&self) -> MatrixMN<f64, Self::MeasurementSize, Self::StateSize>
    where
        DefaultAllocator: Allocator<f64, Self::StateSize, Self::MeasurementSize>;

    /// Returns whether the transmitter and receiver where in line of sight.
    fn visible(&self) -> bool;

    /// Returns the time at which the measurement was performed.
    fn at(&self) -> Epoch;
}

/// A trait to generalize measurement devices such as a ground station
pub trait MeasurementDevice<N>
where
    Self: Sized,
    N: Measurement,
    DefaultAllocator: Allocator<f64, N::StateSize>
        + Allocator<f64, N::StateSize, N::MeasurementSize>
        + Allocator<f64, N::MeasurementSize>
        + Allocator<f64, N::MeasurementSize, N::StateSize>,
{
    type MeasurementInput: Copy;
    fn measure(&self, state: &Self::MeasurementInput) -> N;
}

/// A trait container to specify that given dynamics support linearization, and can be used for state transition matrix computation.
///
/// This trait will likely be made obsolete after the implementation of [#32](https://github.com/ChristopherRabotin/nyx/issues/32).
pub trait AutoDiffDynamics: Dynamics
where
    Self: Sized,
{
    /// Defines the state size of the estimated state
    type HyperStateSize: DimName;
    type STMSize: DimName;

    /// Defines the equations of motion for Dual numbers for these dynamics.
    fn dual_eom(
        &self,
        t: f64,
        state: &VectorN<Hyperdual<f64, Self::HyperStateSize>, Self::STMSize>,
    ) -> (
        VectorN<f64, Self::STMSize>,
        MatrixMN<f64, Self::STMSize, Self::STMSize>,
    )
    where
        DefaultAllocator: Allocator<f64, Self::HyperStateSize>
            + Allocator<f64, Self::STMSize>
            + Allocator<f64, Self::STMSize, Self::STMSize>
            + Allocator<Hyperdual<f64, Self::HyperStateSize>, Self::STMSize>,
        Owned<f64, Self::HyperStateSize>: Copy;

    /// Computes both the state and the gradient of the dynamics. These may be accessed by the
    /// related getters.
    fn compute(
        &self,
        t: f64,
        state: &VectorN<f64, Self::STMSize>,
    ) -> (
        VectorN<f64, Self::STMSize>,
        MatrixMN<f64, Self::STMSize, Self::STMSize>,
    )
    where
        DefaultAllocator: Allocator<f64, Self::HyperStateSize>
            + Allocator<f64, Self::STMSize>
            + Allocator<f64, Self::STMSize, Self::STMSize>
            + Allocator<Hyperdual<f64, Self::HyperStateSize>, Self::STMSize>,
        Owned<f64, Self::HyperStateSize>: Copy,
    {
        let hyperstate = hyperspace_from_vector(&state);

        let (state, grad) = self.dual_eom(t, &hyperstate);

        (state, grad)
    }
}

/// Specifies the format of the Epoch during serialization
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EpochFormat {
    /// Default is GregorianUtc, as defined in [hifitime](https://docs.rs/hifitime/).
    GregorianUtc,
    GregorianTai,
    MjdTai,
    MjdTt,
    MjdUtc,
    JdeEt,
    JdeTai,
    JdeTt,
    JdeUtc,
    /// Seconds past a provided TAI Epoch
    TaiSecs(f64),
    /// Days past a provided TAI Epoch
    TaiDays(f64),
}

impl fmt::Display for EpochFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EpochFormat::GregorianUtc => write!(f, "Gregorian UTC"),
            EpochFormat::GregorianTai => write!(f, "Gregorian TAI"),
            EpochFormat::MjdTai => write!(f, "MJD TAI"),
            EpochFormat::MjdTt => write!(f, "MJD TT"),
            EpochFormat::MjdUtc => write!(f, "MJD UTC"),
            EpochFormat::JdeEt => write!(f, "JDE ET"),
            EpochFormat::JdeTai => write!(f, "JDE TAI"),
            EpochFormat::JdeTt => write!(f, "JDE TT"),
            EpochFormat::JdeUtc => write!(f, "JDE UTC"),
            EpochFormat::TaiSecs(_) => write!(f, "TAI+ s"),
            EpochFormat::TaiDays(_) => write!(f, "TAI+ days"),
        }
    }
}

/// Specifies the format of the covariance during serialization
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CovarFormat {
    /// Default: allows plotting the variance of the elements instead of the covariance
    Sqrt,
    /// Keeps the covariance as computed, i.e. one sigma (~68%), causes e.g. positional elements in km^2.
    Sigma1,
    /// Three sigma covers about 99.7% of the distribution
    Sigma3,
    /// Allows specifying a custom multiplication factor of each element of the covariance.
    MulSigma(f64),
}

impl fmt::Display for CovarFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CovarFormat::Sqrt => write!(f, "exptd_val_"),
            CovarFormat::Sigma1 => write!(f, "covar_"),
            CovarFormat::Sigma3 => write!(f, "3sig_covar"),
            CovarFormat::MulSigma(x) => write!(f, "{}sig_covar", x),
        }
    }
}
