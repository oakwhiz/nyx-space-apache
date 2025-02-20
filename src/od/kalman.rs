use crate::dimensions::allocator::Allocator;
use crate::dimensions::{DefaultAllocator, DimName, MatrixMN, VectorN, U3};

pub use super::estimate::{Estimate, KfEstimate};
pub use super::residual::Residual;
pub use super::snc::SNC;
use super::{CovarFormat, EpochFormat, Filter, State};
pub use crate::errors::NyxError;

/// Defines both a Classical and an Extended Kalman filter (CKF and EKF)
/// S: State size (not propagated vector size)
/// A: Acceleration size (for SNC)
/// M: Measurement size (used for the sensitivity matrix)
/// T: Type of state
/// P: Propagated vector size
#[derive(Debug, Clone)]
pub struct KF<T, A, M>
where
    A: DimName,
    M: DimName,
    T: State,
    DefaultAllocator: Allocator<f64, M>
        + Allocator<f64, <T as State>::Size>
        + Allocator<f64, A>
        + Allocator<f64, M, M>
        + Allocator<f64, M, <T as State>::Size>
        + Allocator<f64, <T as State>::Size, <T as State>::Size>
        + Allocator<f64, A, A>
        + Allocator<f64, <T as State>::Size, A>
        + Allocator<f64, A, <T as State>::Size>
        + Allocator<usize, <T as State>::Size>
        + Allocator<usize, <T as State>::Size, <T as State>::Size>,
{
    /// The previous estimate used in the KF computations.
    pub prev_estimate: KfEstimate<T>,
    /// Sets the Measurement noise (usually noted R)
    pub measurement_noise: MatrixMN<f64, M, M>,
    /// A sets of process noise (usually noted Q), must be ordered chronologically
    pub process_noise: Vec<SNC<A>>,
    /// Determines whether this KF should operate as a Conventional/Classical Kalman filter or an Extended Kalman Filter.
    /// Recall that one should switch to an Extended KF only once the estimate is good (i.e. after a few good measurement updates on a CKF).
    pub ekf: bool,
    h_tilde: MatrixMN<f64, M, <T as State>::Size>,
    stm: MatrixMN<f64, <T as State>::Size, <T as State>::Size>,
    stm_updated: bool,
    h_tilde_updated: bool,
    epoch_fmt: EpochFormat, // Stored here only for simplification, kinda ugly
    covar_fmt: CovarFormat, // Idem
    prev_used_snc: usize,
}

impl<T, A, M> KF<T, A, M>
where
    A: DimName,
    M: DimName,
    T: State,
    DefaultAllocator: Allocator<f64, M>
        + Allocator<f64, <T as State>::Size>
        + Allocator<f64, A>
        + Allocator<f64, M, M>
        + Allocator<f64, M, <T as State>::Size>
        + Allocator<f64, <T as State>::Size, M>
        + Allocator<f64, <T as State>::Size, <T as State>::Size>
        + Allocator<f64, A, A>
        + Allocator<f64, <T as State>::Size, A>
        + Allocator<f64, A, <T as State>::Size>
        + Allocator<usize, <T as State>::Size>
        + Allocator<usize, <T as State>::Size, <T as State>::Size>,
{
    /// Initializes this KF with an initial estimate, measurement noise, and one process noise
    pub fn new(
        initial_estimate: KfEstimate<T>,
        process_noise: SNC<A>,
        measurement_noise: MatrixMN<f64, M, M>,
    ) -> Self {
        let epoch_fmt = initial_estimate.epoch_fmt;
        let covar_fmt = initial_estimate.covar_fmt;

        assert_eq!(
            A::dim() % 3,
            0,
            "SNC can only be applied to accelerations multiple of 3"
        );

        // Set the initial epoch of the SNC
        let mut process_noise = process_noise;
        process_noise.init_epoch = Some(initial_estimate.epoch());

        Self {
            prev_estimate: initial_estimate,
            measurement_noise,
            process_noise: vec![process_noise],
            ekf: false,
            h_tilde: MatrixMN::<f64, M, <T as State>::Size>::zeros(),
            stm: MatrixMN::<f64, <T as State>::Size, <T as State>::Size>::identity(),
            stm_updated: false,
            h_tilde_updated: false,
            epoch_fmt,
            covar_fmt,
            prev_used_snc: 0,
        }
    }

    /// Initializes this KF with an initial estimate, measurement noise, and several process noise
    /// WARNING: SNCs MUST be ordered chronologically! They will be selected automatically by walking
    /// the list of SNCs backward until one can be applied!
    pub fn with_sncs(
        initial_estimate: KfEstimate<T>,
        process_noises: Vec<SNC<A>>,
        measurement_noise: MatrixMN<f64, M, M>,
    ) -> Self {
        let epoch_fmt = initial_estimate.epoch_fmt;
        let covar_fmt = initial_estimate.covar_fmt;

        assert_eq!(
            A::dim() % 3,
            0,
            "SNC can only be applied to accelerations multiple of 3"
        );
        let mut process_noises = process_noises;
        // Set the initial epoch of the SNC
        for snc in &mut process_noises {
            snc.init_epoch = Some(initial_estimate.epoch());
        }

        Self {
            prev_estimate: initial_estimate,
            measurement_noise,
            process_noise: process_noises,
            ekf: false,
            h_tilde: MatrixMN::<f64, M, <T as State>::Size>::zeros(),
            stm: MatrixMN::<f64, <T as State>::Size, <T as State>::Size>::identity(),
            stm_updated: false,
            h_tilde_updated: false,
            epoch_fmt,
            covar_fmt,
            prev_used_snc: 0,
        }
    }
}

impl<T, M> KF<T, U3, M>
where
    M: DimName,
    T: State,
    DefaultAllocator: Allocator<f64, M>
        + Allocator<f64, <T as State>::Size>
        + Allocator<f64, M, M>
        + Allocator<f64, M, <T as State>::Size>
        + Allocator<f64, <T as State>::Size, M>
        + Allocator<f64, <T as State>::Size, <T as State>::Size>
        + Allocator<f64, U3, U3>
        + Allocator<f64, <T as State>::Size, U3>
        + Allocator<f64, U3, <T as State>::Size>
        + Allocator<usize, <T as State>::Size>
        + Allocator<usize, <T as State>::Size, <T as State>::Size>,
{
    /// Initializes this KF without SNC
    pub fn no_snc(initial_estimate: KfEstimate<T>, measurement_noise: MatrixMN<f64, M, M>) -> Self {
        let epoch_fmt = initial_estimate.epoch_fmt;
        let covar_fmt = initial_estimate.covar_fmt;
        Self {
            prev_estimate: initial_estimate,
            measurement_noise,
            process_noise: Vec::new(),
            ekf: false,
            h_tilde: MatrixMN::<f64, M, <T as State>::Size>::zeros(),
            stm: MatrixMN::<f64, <T as State>::Size, <T as State>::Size>::identity(),
            stm_updated: false,
            h_tilde_updated: false,
            epoch_fmt,
            covar_fmt,
            prev_used_snc: 0,
        }
    }
}

impl<T, A, M> Filter<T, A, M> for KF<T, A, M>
where
    A: DimName,
    M: DimName,
    T: State,
    DefaultAllocator: Allocator<f64, M>
        + Allocator<f64, <T as State>::Size>
        + Allocator<f64, A>
        + Allocator<f64, M, M>
        + Allocator<f64, M, <T as State>::Size>
        + Allocator<f64, <T as State>::Size, M>
        + Allocator<f64, <T as State>::Size, <T as State>::Size>
        + Allocator<f64, A, A>
        + Allocator<f64, <T as State>::Size, A>
        + Allocator<f64, A, <T as State>::Size>
        + Allocator<usize, <T as State>::Size>
        + Allocator<usize, <T as State>::Size, <T as State>::Size>,
{
    type Estimate = KfEstimate<T>;

    /// Returns the previous estimate
    fn previous_estimate(&self) -> &Self::Estimate {
        &self.prev_estimate
    }

    fn set_previous_estimate(&mut self, est: &Self::Estimate) {
        self.prev_estimate = est.clone();
    }

    /// Update the State Transition Matrix (STM). This function **must** be called in between each
    /// call to `time_update` or `measurement_update`.
    fn update_stm(&mut self, new_stm: MatrixMN<f64, <T as State>::Size, <T as State>::Size>) {
        self.stm = new_stm;
        self.stm_updated = true;
    }

    /// Update the sensitivity matrix (or "H tilde"). This function **must** be called prior to each
    /// call to `measurement_update`.
    fn update_h_tilde(&mut self, h_tilde: MatrixMN<f64, M, <T as State>::Size>) {
        self.h_tilde = h_tilde;
        self.h_tilde_updated = true;
    }

    /// Computes a time update/prediction (i.e. advances the filter estimate with the updated STM).
    ///
    /// May return a FilterError if the STM was not updated.
    fn time_update(&mut self, nominal_state: T) -> Result<Self::Estimate, NyxError> {
        if !self.stm_updated {
            return Err(NyxError::StateTransitionMatrixNotUpdated);
        }

        let covar_bar = &self.stm * &self.prev_estimate.covar * &self.stm.transpose();

        let state_bar = if self.ekf {
            VectorN::<f64, <T as State>::Size>::zeros()
        } else {
            &self.stm * &self.prev_estimate.state_deviation
        };
        let estimate = KfEstimate {
            nominal_state,
            state_deviation: state_bar,
            covar: covar_bar.clone(),
            covar_bar,
            stm: self.stm.clone(),
            predicted: true,
            epoch_fmt: self.epoch_fmt,
            covar_fmt: self.covar_fmt,
        };
        self.stm_updated = false;
        self.prev_estimate = estimate.clone();
        // Update the prev epoch for all SNCs
        for snc in &mut self.process_noise {
            snc.prev_epoch = Some(self.prev_estimate.epoch());
        }
        Ok(estimate)
    }

    /// Computes the measurement update with a provided real observation and computed observation.
    ///
    /// May return a FilterError if the STM or sensitivity matrices were not updated.
    fn measurement_update(
        &mut self,
        nominal_state: T,
        real_obs: &VectorN<f64, M>,
        computed_obs: &VectorN<f64, M>,
    ) -> Result<(Self::Estimate, Residual<M>), NyxError> {
        if !self.stm_updated {
            return Err(NyxError::StateTransitionMatrixNotUpdated);
        }
        if !self.h_tilde_updated {
            return Err(NyxError::SensitivityNotUpdated);
        }

        let mut covar_bar = &self.stm * &self.prev_estimate.covar * &self.stm.transpose();
        // Try to apply an SNC, if applicable
        for (i, snc) in self.process_noise.iter().enumerate().rev() {
            if let Some(snc_matrix) = snc.to_matrix(nominal_state.epoch()) {
                // Check if we're using another SNC than the one before
                if self.prev_used_snc != i {
                    info!("Switched to {}-th {}", i, snc);
                    self.prev_used_snc = i;
                }

                // Let's compute the Gamma matrix, an approximation of the time integral
                // which assumes that the acceleration is constant between these two measurements.
                let mut gamma = MatrixMN::<f64, <T as State>::Size, A>::zeros();
                let delta_t = (nominal_state.epoch() - self.prev_estimate.epoch()).in_seconds();
                for blk in 0..A::dim() / 3 {
                    for i in 0..3 {
                        let idx_i = i + A::dim() * blk;
                        let idx_j = i + 3 * blk;
                        let idx_k = i + 3 + A::dim() * blk;
                        // For first block
                        // (0, 0) (1, 1) (2, 2) <=> \Delta t^2/2
                        // (3, 0) (4, 1) (5, 2) <=> \Delta t
                        // Second block
                        // (6, 3) (7, 4) (8, 5) <=> \Delta t^2/2
                        // (9, 3) (10, 4) (11, 5) <=> \Delta t
                        // * \Delta t^2/2
                        // (i, i) when blk = 0
                        // (i + A::dim() * blk, i + 3) when blk = 1
                        // (i + A::dim() * blk, i + 3 * blk)
                        // * \Delta t
                        // (i + 3, i) when blk = 0
                        // (i + 3, i + 9) when blk = 1 (and I think i + 12 + 3)
                        // (i + 3 + A::dim() * blk, i + 3 * blk)
                        gamma[(idx_i, idx_j)] = delta_t.powi(2) / 2.0;
                        gamma[(idx_k, idx_j)] = delta_t;
                    }
                }
                // Let's add the process noise
                covar_bar += &gamma * snc_matrix * &gamma.transpose();
                // And break so we don't add any more process noise
                break;
            }
        }

        let h_tilde_t = &self.h_tilde.transpose();
        let mut invertible_part = &self.h_tilde * &covar_bar * h_tilde_t + &self.measurement_noise;
        if !invertible_part.try_inverse_mut() {
            return Err(NyxError::SingularKalmanGain);
        }

        let gain = &covar_bar * h_tilde_t * &invertible_part;

        // Compute observation deviation (usually marked as y_i)
        let prefit = real_obs - computed_obs;

        // Compute the state estimate
        let (state_hat, res) = if self.ekf {
            let state_hat = &gain * &prefit;
            let postfit = &prefit - (&self.h_tilde * &state_hat);
            (
                state_hat,
                Residual::new(nominal_state.epoch(), prefit, postfit),
            )
        } else {
            // Must do a time update first
            let state_bar = &self.stm * &self.prev_estimate.state_deviation;
            let postfit = &prefit - (&self.h_tilde * &state_bar);
            (
                state_bar + &gain * &postfit,
                Residual::new(nominal_state.epoch(), prefit, postfit),
            )
        };

        // Compute covariance (Joseph update)
        let first_term = MatrixMN::<f64, <T as State>::Size, <T as State>::Size>::identity()
            - &gain * &self.h_tilde;
        let covar = &first_term * &covar_bar * &first_term.transpose()
            + &gain * &self.measurement_noise * &gain.transpose();

        // And wrap up
        let estimate = KfEstimate {
            nominal_state,
            state_deviation: state_hat,
            covar,
            covar_bar,
            stm: self.stm.clone(),
            predicted: false,
            epoch_fmt: self.epoch_fmt,
            covar_fmt: self.covar_fmt,
        };

        self.stm_updated = false;
        self.h_tilde_updated = false;
        self.prev_estimate = estimate.clone();
        Ok((estimate, res))
    }

    fn is_extended(&self) -> bool {
        self.ekf
    }

    fn set_extended(&mut self, status: bool) {
        self.ekf = status;
    }

    /// Overwrites all of the process noises to the one provided
    fn set_process_noise(&mut self, snc: SNC<A>) {
        self.process_noise = vec![snc];
    }
}
