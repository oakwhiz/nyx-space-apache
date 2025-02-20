use crate::dimensions::allocator::Allocator;
use crate::dimensions::{DefaultAllocator, DimName, VectorN, U3};

// This determines when to take into consideration the magnitude of the state_delta and
// prevents dividing by too small of a number.
const REL_ERR_THRESH: f64 = 0.1;

/// The Error Control trait manages how a propagator computes the error in the current step.
pub trait ErrorCtrl
where
    Self: Copy,
{
    /// Computes the actual error of the current step.
    ///
    /// The `error_est` is the estimated error computed from the difference in the two stages of
    /// of the RK propagator. The `candidate` variable is the candidate state, and `cur_state` is
    /// the current state. This function must return the error.
    fn estimate<N: DimName>(
        error_est: &VectorN<f64, N>,
        candidate: &VectorN<f64, N>,
        cur_state: &VectorN<f64, N>,
    ) -> f64
    where
        DefaultAllocator: Allocator<f64, N>;
}

/// A largest error control which effectively computes the largest error at each component
///
/// This is a standard error computation algorithm, but it's argubly bad if the state's components have different units.
/// It calculates the largest local estimate of the error from the integration (`error_est`)
/// given the difference in the candidate state and the previous state (`state_delta`).
/// This error estimator is from the physical model estimator of GMAT
/// (Source)[https://github.com/ChristopherRabotin/GMAT/blob/37201a6290e7f7b941bc98ee973a527a5857104b/src/base/forcemodel/PhysicalModel.cpp#L987]
#[derive(Clone, Copy)]
pub struct LargestError;
impl ErrorCtrl for LargestError {
    fn estimate<N: DimName>(
        error_est: &VectorN<f64, N>,
        candidate: &VectorN<f64, N>,
        cur_state: &VectorN<f64, N>,
    ) -> f64
    where
        DefaultAllocator: Allocator<f64, N>,
    {
        let state_delta = candidate - cur_state;
        let mut max_err = 0.0;
        for (i, prop_err_i) in error_est.iter().enumerate() {
            let err = if state_delta[i] > REL_ERR_THRESH {
                (prop_err_i / state_delta[i]).abs()
            } else {
                prop_err_i.abs()
            };
            if err > max_err {
                max_err = err;
            }
        }
        max_err
    }
}

/// A largest step error control which effectively computes the L1 norm of the provided Vector of size 3
///
/// Note that this error controller should be preferrably be used only with slices of a state with the same units.
/// For example, one should probably use this for position independently of using it for the velocity.
/// (Source)[https://github.com/ChristopherRabotin/GMAT/blob/37201a6290e7f7b941bc98ee973a527a5857104b/src/base/forcemodel/ODEModel.cpp#L3033]
#[derive(Clone, Copy)]
pub struct LargestStep;
impl ErrorCtrl for LargestStep {
    fn estimate<N: DimName>(
        error_est: &VectorN<f64, N>,
        candidate: &VectorN<f64, N>,
        cur_state: &VectorN<f64, N>,
    ) -> f64
    where
        DefaultAllocator: Allocator<f64, N>,
    {
        let state_delta = candidate - cur_state;
        let mut mag = 0.0f64;
        let mut err = 0.0f64;
        for i in 0..N::dim() {
            mag += state_delta[i].abs();
            err += error_est[i].abs();
        }

        if mag > REL_ERR_THRESH {
            err / mag
        } else {
            err
        }
    }
}

/// A largest state error control
///
/// (Source)[https://github.com/ChristopherRabotin/GMAT/blob/37201a6290e7f7b941bc98ee973a527a5857104b/src/base/forcemodel/ODEModel.cpp#L3018]
#[derive(Clone, Copy)]
pub struct LargestState;
impl ErrorCtrl for LargestState {
    fn estimate<N: DimName>(
        error_est: &VectorN<f64, N>,
        candidate: &VectorN<f64, N>,
        cur_state: &VectorN<f64, N>,
    ) -> f64
    where
        DefaultAllocator: Allocator<f64, N>,
    {
        let sum_state = candidate + cur_state;
        let mut mag = 0.0f64;
        let mut err = 0.0f64;
        for i in 0..N::dim() {
            mag += 0.5 * sum_state[i].abs();
            err += error_est[i].abs();
        }

        if mag > REL_ERR_THRESH {
            err / mag
        } else {
            err
        }
    }
}

/// An RSS step error control which effectively computes the L2 norm of the provided Vector of size 3
///
/// Note that this error controller should be preferrably be used only with slices of a state with the same units.
/// For example, one should probably use this for position independently of using it for the velocity.
/// (Source)[https://github.com/ChristopherRabotin/GMAT/blob/37201a6290e7f7b941bc98ee973a527a5857104b/src/base/forcemodel/ODEModel.cpp#L3045]
#[derive(Clone, Copy)]
pub struct RSSStep;
impl ErrorCtrl for RSSStep {
    fn estimate<N: DimName>(
        error_est: &VectorN<f64, N>,
        candidate: &VectorN<f64, N>,
        cur_state: &VectorN<f64, N>,
    ) -> f64
    where
        DefaultAllocator: Allocator<f64, N>,
    {
        let mag = (candidate - cur_state).norm();
        let err = error_est.norm();
        if mag > REL_ERR_THRESH {
            err / mag
        } else {
            err
        }
    }
}

/// An RSS state error control: when in doubt, use this error controller, especially for high accurracy.
///
/// Here is the warning from GMAT R2016a on this error controller:
/// > This is a more stringent error control method than [`rss_step`] that is often used as the default in other software such as STK.
/// > If you set [the] accuracy to a very small number, 1e-13 for example, and set  the error control to [`rss_step`], integrator
/// > performance will be poor, for little if any improvement in the accuracy of the orbit integration.
/// For more best practices of these integrators (which clone those in GMAT), please refer to the
/// [GMAT reference](https://github.com/ChristopherRabotin/GMAT/blob/37201a6290e7f7b941bc98ee973a527a5857104b/doc/help/src/Resource_NumericalIntegrators.xml#L1292).
/// (Source)[https://github.com/ChristopherRabotin/GMAT/blob/37201a6290e7f7b941bc98ee973a527a5857104b/src/base/forcemodel/ODEModel.cpp#L3004]
#[derive(Clone, Copy)]
pub struct RSSState;
impl ErrorCtrl for RSSState {
    fn estimate<N: DimName>(
        error_est: &VectorN<f64, N>,
        candidate: &VectorN<f64, N>,
        cur_state: &VectorN<f64, N>,
    ) -> f64
    where
        DefaultAllocator: Allocator<f64, N>,
    {
        let mag = 0.5 * (candidate + cur_state).norm();
        let err = error_est.norm();
        if mag > REL_ERR_THRESH {
            err / mag
        } else {
            err
        }
    }
}

/// An RSS state error control which effectively for the provided vector
/// composed of two vectors of the same unit, both of size 3 (e.g. position + velocity).
#[derive(Clone, Copy)]
pub struct RSSStatePV;
impl ErrorCtrl for RSSStatePV {
    fn estimate<N: DimName>(
        error_est: &VectorN<f64, N>,
        candidate: &VectorN<f64, N>,
        cur_state: &VectorN<f64, N>,
    ) -> f64
    where
        DefaultAllocator: Allocator<f64, N>,
    {
        let err_radius = RSSState::estimate::<U3>(
            &error_est.fixed_rows::<U3>(0).into_owned(),
            &candidate.fixed_rows::<U3>(0).into_owned(),
            &cur_state.fixed_rows::<U3>(0).into_owned(),
        );
        let err_velocity = RSSState::estimate::<U3>(
            &error_est.fixed_rows::<U3>(3).into_owned(),
            &candidate.fixed_rows::<U3>(3).into_owned(),
            &cur_state.fixed_rows::<U3>(3).into_owned(),
        );

        if err_radius > err_velocity {
            err_radius
        } else {
            err_velocity
        }
    }
}

/// An RSS state error control which effectively for the provided vector
/// composed of two vectors of the same unit, both of size 3 (e.g. position + velocity).
#[derive(Clone, Copy)]
pub struct RSSStepPV;
impl ErrorCtrl for RSSStepPV {
    fn estimate<N: DimName>(
        error_est: &VectorN<f64, N>,
        candidate: &VectorN<f64, N>,
        cur_state: &VectorN<f64, N>,
    ) -> f64
    where
        DefaultAllocator: Allocator<f64, N>,
    {
        let err_radius = RSSStep::estimate::<U3>(
            &error_est.fixed_rows::<U3>(0).into_owned(),
            &candidate.fixed_rows::<U3>(0).into_owned(),
            &cur_state.fixed_rows::<U3>(0).into_owned(),
        );
        let err_velocity = RSSStep::estimate::<U3>(
            &error_est.fixed_rows::<U3>(3).into_owned(),
            &candidate.fixed_rows::<U3>(3).into_owned(),
            &cur_state.fixed_rows::<U3>(3).into_owned(),
        );

        if err_radius > err_velocity {
            err_radius
        } else {
            err_velocity
        }
    }
}

/// An RSS state error control which effectively for the provided vector
/// composed of two vectors of the same unit, both of size 3 (e.g. position + velocity).
#[derive(Clone, Copy)]
pub struct RSSStepPVStm;
impl ErrorCtrl for RSSStepPVStm {
    fn estimate<N: DimName>(
        error_est: &VectorN<f64, N>,
        candidate: &VectorN<f64, N>,
        cur_state: &VectorN<f64, N>,
    ) -> f64
    where
        DefaultAllocator: Allocator<f64, N>,
    {
        let err_radius = RSSStep::estimate::<U3>(
            &error_est.fixed_rows::<U3>(0).into_owned(),
            &candidate.fixed_rows::<U3>(0).into_owned(),
            &cur_state.fixed_rows::<U3>(0).into_owned(),
        );
        let err_velocity = RSSStep::estimate::<U3>(
            &error_est.fixed_rows::<U3>(3).into_owned(),
            &candidate.fixed_rows::<U3>(3).into_owned(),
            &cur_state.fixed_rows::<U3>(3).into_owned(),
        );
        let err_cov_radius = RSSStep::estimate::<U3>(
            &VectorN::<f64, U3>::new(error_est[6], error_est[6 + 7], error_est[6 + 14]),
            &VectorN::<f64, U3>::new(candidate[6], candidate[6 + 7], candidate[6 + 14]),
            &VectorN::<f64, U3>::new(cur_state[6], cur_state[6 + 7], cur_state[6 + 14]),
        );

        let err_cov_velocity = RSSStep::estimate::<U3>(
            &VectorN::<f64, U3>::new(error_est[6 + 21], error_est[6 + 28], error_est[6 + 35]),
            &VectorN::<f64, U3>::new(candidate[6 + 21], candidate[6 + 28], candidate[6 + 35]),
            &VectorN::<f64, U3>::new(cur_state[6 + 21], cur_state[6 + 28], cur_state[6 + 35]),
        );

        let errs = vec![err_radius, err_velocity, err_cov_radius, err_cov_velocity];
        let mut max_err = 0.0;
        for err in errs {
            if err > max_err {
                max_err = err;
            }
        }

        max_err
    }
}
