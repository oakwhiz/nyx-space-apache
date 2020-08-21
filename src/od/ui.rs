use crate::dimensions::allocator::Allocator;
use crate::dimensions::{DefaultAllocator, DimName};

pub use super::estimate::*;
pub use super::kalman::*;
pub use super::ranging::*;
pub use super::residual::*;
pub use super::snc::*;
pub use super::srif::*;
pub use super::*;

use crate::propagators::error_ctrl::ErrorCtrl;
use crate::propagators::Propagator;

use std::marker::PhantomData;
use std::sync::mpsc::channel;

/// An orbit determination process. Note that everything passed to this structure is moved.
pub struct ODProcess<
    'a,
    D: Estimable<MsrIn, LinStateSize = Msr::StateSize>,
    E: ErrorCtrl,
    Msr: Measurement,
    N: MeasurementDevice<MsrIn, Msr>,
    T: EkfTrigger,
    A: DimName,
    K: Filter<D::LinStateSize, A, Msr::MeasurementSize, D::StateType>,
    MsrIn,
> where
    D::StateType: EstimableState<Msr::StateSize>,
    DefaultAllocator: Allocator<f64, D::StateSize>
        + Allocator<f64, Msr::MeasurementSize>
        + Allocator<f64, Msr::MeasurementSize, Msr::StateSize>
        + Allocator<f64, Msr::StateSize>
        + Allocator<f64, Msr::MeasurementSize, Msr::MeasurementSize>
        + Allocator<f64, Msr::MeasurementSize, D::LinStateSize>
        + Allocator<f64, D::LinStateSize, Msr::MeasurementSize>
        + Allocator<f64, D::LinStateSize, D::LinStateSize>
        + Allocator<f64, A>
        + Allocator<f64, A, A>
        + Allocator<f64, D::LinStateSize, A>
        + Allocator<f64, A, D::LinStateSize>,
{
    /// Propagator used for the estimation
    pub prop: Propagator<'a, D, E>,
    /// Kalman filter itself
    pub kf: K,
    /// List of measurement devices used
    pub devices: Vec<N>,
    /// Whether or not these devices can make simultaneous measurements of the spacecraft
    pub simultaneous_msr: bool,
    /// Vector of estimates available after a pass
    pub estimates: Vec<K::Estimate>,
    /// Vector of residuals available after a pass
    pub residuals: Vec<Residual<Msr::MeasurementSize>>,
    pub ekf_trigger: T,
    _marker: PhantomData<A>,
}

impl<
        'a,
        D: Estimable<MsrIn, LinStateSize = Msr::StateSize>,
        E: ErrorCtrl,
        Msr: Measurement,
        N: MeasurementDevice<MsrIn, Msr>,
        T: EkfTrigger,
        A: DimName,
        K: Filter<D::LinStateSize, A, Msr::MeasurementSize, D::StateType>,
        MsrIn,
    > ODProcess<'a, D, E, Msr, N, T, A, K, MsrIn>
where
    D::StateType: EstimableState<Msr::StateSize>,
    DefaultAllocator: Allocator<f64, D::StateSize>
        + Allocator<f64, Msr::MeasurementSize>
        + Allocator<f64, Msr::MeasurementSize, Msr::StateSize>
        + Allocator<f64, Msr::StateSize>
        + Allocator<f64, Msr::MeasurementSize, Msr::MeasurementSize>
        + Allocator<f64, Msr::MeasurementSize, D::LinStateSize>
        + Allocator<f64, D::LinStateSize, Msr::MeasurementSize>
        + Allocator<f64, D::LinStateSize, D::LinStateSize>
        + Allocator<f64, A>
        + Allocator<f64, A, A>
        + Allocator<f64, D::LinStateSize, A>
        + Allocator<f64, A, D::LinStateSize>,
{
    pub fn ekf(
        prop: Propagator<'a, D, E>,
        kf: K,
        devices: Vec<N>,
        simultaneous_msr: bool,
        num_expected_msr: usize,
        trigger: T,
    ) -> Self {
        let mut estimates = Vec::with_capacity(num_expected_msr + 1);
        estimates.push(kf.previous_estimate().clone());
        Self {
            prop,
            kf,
            devices,
            simultaneous_msr,
            estimates,
            residuals: Vec::with_capacity(num_expected_msr),
            ekf_trigger: trigger,
            _marker: PhantomData::<A>,
        }
    }

    pub fn default_ekf(prop: Propagator<'a, D, E>, kf: K, devices: Vec<N>, trigger: T) -> Self {
        let mut estimates = Vec::with_capacity(10_001);
        estimates.push(kf.previous_estimate().clone());
        Self {
            prop,
            kf,
            devices,
            simultaneous_msr: false,
            estimates,
            residuals: Vec::with_capacity(10_000),
            ekf_trigger: trigger,
            _marker: PhantomData::<A>,
        }
    }

    /// Allows to smooth the provided estimates. Returns the smoothed estimates or an error.
    ///
    /// Estimates must be ordered in chronological order. This function will smooth the
    /// estimates from the last in the list to the first one.
    pub fn smooth(&mut self) -> Result<Vec<K::Estimate>, FilterError> {
        let num = self.estimates.len() - 1;
        let mut k = num - 1;

        info!("Smoothing {} estimates", num + 1);
        let mut smoothed = Vec::with_capacity(num + 1);
        // Set the first item of the smoothed estimates to the last estimate (we cannot smooth the very last estimate)
        smoothed.push(self.estimates[k].clone());

        // Note: we're using `!=` because Rust ensures that k can never be negative. ("comparison is useless due to type limits").
        loop {
            // Borrow the previously smoothed estimate of the k+1 estimate
            let sm_k_kp1 = &smoothed[num - k - 1];
            // Borrow the k-th estimate, which we're smoothing with the next estimate
            let est_k = &self.estimates[k];
            // Borrow the k-th estimate, which we're smoothing with the next estimate
            let est_kp1 = &self.estimates[k + 1];
            // Clone and invert the STM \phi(k -> k+1) (this is the same STM as the estimate at k+1)
            let mut stm_kp1_k = sm_k_kp1.stm().clone();
            if !stm_kp1_k.try_inverse_mut() {
                return Err(FilterError::StateTransitionMatrixSingular);
            }
            // Invert the p_k+1 covar knowing only k
            let mut p_kp1_inv = est_kp1.covar().clone();
            if !p_kp1_inv.try_inverse_mut() {
                return Err(FilterError::CovarianceMatrixSingular);
            }
            // Compute Sk
            let s_k = est_k.covar() * stm_kp1_k.transpose() * p_kp1_inv;
            let mut smoothed_est_k = est_k.clone();
            // Compute the smoothed state deviation
            smoothed_est_k.set_state_deviation(
                est_k.state_deviation()
                    + &s_k * (sm_k_kp1.state_deviation() - stm_kp1_k * est_k.state_deviation()),
            );
            // Compute the smoothed covariance
            smoothed_est_k.set_covar(
                est_k.covar() + &s_k * (sm_k_kp1.covar() - est_kp1.covar()) * &s_k.transpose(),
            );
            // Move on
            smoothed.push(smoothed_est_k);
            if k == 0 {
                break;
            }
            k -= 1;
        }

        // And reverse to maintain the order of estimates
        smoothed.reverse();
        Ok(smoothed)
    }

    /// Allows iterating on the filter solution
    pub fn iterate(&mut self, measurements: &[Msr]) -> Option<FilterError> {
        // First, smooth the estimates
        let smoothed = match self.smooth() {
            Ok(smoothed) => smoothed,
            Err(e) => return Some(e),
        };
        // Get the first estimate post-smoothing
        let mut init_smoothed = smoothed[0].clone();
        println!("{}", init_smoothed.epoch().as_gregorian_tai_str());
        // Reset the propagator
        self.prop.reset();
        let mut iterated_state = self.prop.dynamics.state_vector();
        for (i, x) in init_smoothed.state_deviation().iter().enumerate() {
            iterated_state[i] += x;
        }
        self.prop
            .dynamics
            .set_state(self.prop.dynamics.time(), &iterated_state);
        // Set the filter's initial state to this smoothed estimate
        init_smoothed.set_state_deviation(VectorN::<f64, Msr::StateSize>::zeros());
        self.kf.set_previous_estimate(&init_smoothed);
        // And re-run the filter
        self.process_measurements(measurements)?;
        None
    }

    /// Allows processing all measurements with covariance mapping.
    ///
    /// Important notes:
    /// + the measurements have be to mapped to a fixed time corresponding to the step of the propagator
    pub fn process_measurements(&mut self, measurements: &[Msr]) -> Option<FilterError> {
        let (tx, rx) = channel();
        self.prop.tx_chan = Some(tx);
        assert!(
            !measurements.is_empty(),
            "must have at least one measurement"
        );
        // Start by propagating the estimator (on the same thread).
        let num_msrs = measurements.len();

        let prop_time = measurements[num_msrs - 1].epoch() - self.kf.previous_estimate().epoch();
        info!(
            "Navigation propagating for a total of {} seconds (~ {:.3} days)",
            prop_time,
            prop_time / 86_400.0
        );

        // Push the initial estimate
        let prev = self.kf.previous_estimate().clone();
        let mut prev_dt = prev.epoch();

        let mut reported = vec![false; 11];
        let mut arc_warned = false;

        info!(
            "Processing {} measurements with covariance mapping",
            num_msrs
        );

        for (msr_cnt, msr) in measurements.iter().enumerate() {
            let next_msr_epoch = msr.epoch();

            let delta_t = next_msr_epoch - prev_dt;
            self.prop.until_time_elapsed(delta_t);

            while let Ok(nominal_state) = rx.try_recv() {
                // Get the datetime and info needed to compute the theoretical measurement according to the model
                let meas_input = self.prop.dynamics.to_measurement(&nominal_state);
                let dt = nominal_state.epoch();

                // Update the STM of the KF (needed between each measurement or time update)
                let stm = self.prop.dynamics.extract_stm(&nominal_state);
                self.kf.update_stm(stm);

                // Check if we should do a time update or a measurement update
                if next_msr_epoch > dt {
                    if msr_cnt == 0 && !arc_warned {
                        warn!("OD arc starts prior to first measurement");
                        arc_warned = true;
                    }
                    // No measurement can be used here, let's just do a time update
                    debug!("time update {}", dt.as_gregorian_tai_str());
                    match self.kf.time_update(nominal_state) {
                        Ok(est) => {
                            if self.kf.is_extended() {
                                self.prop.dynamics.set_estimated_state(
                                    self.prop.dynamics.estimated_state() + est.state_deviation(),
                                );
                            }
                            self.estimates.push(est);
                        }
                        Err(e) => return Some(e),
                    }
                } else {
                    // The epochs match, so this is a valid measurement to use
                    // Get the computed observations
                    for device in self.devices.iter() {
                        if let Some(computed_meas) = device.measure(&meas_input) {
                            if computed_meas.visible() {
                                self.kf.update_h_tilde(computed_meas.sensitivity());

                                // Switch back from extended if necessary
                                if self.kf.is_extended() && self.ekf_trigger.disable_ekf(dt) {
                                    self.kf.set_extended(false);
                                    info!("EKF disabled @ {}", dt.as_gregorian_tai_str());
                                }

                                match self.kf.measurement_update(
                                    nominal_state,
                                    msr.observation(),
                                    computed_meas.observation(),
                                ) {
                                    Ok((est, res)) => {
                                        debug!(
                                            "msr update msr #{} {}",
                                            msr_cnt,
                                            dt.as_gregorian_tai_str()
                                        );

                                        // Switch to EKF if necessary, and update the dynamics and such
                                        // Note: we call enable_ekf first to ensure that the trigger gets
                                        // called in case it needs to save some information (e.g. the
                                        // StdEkfTrigger needs to store the time of the previous measurement).
                                        if self.ekf_trigger.enable_ekf(&est)
                                            && !self.kf.is_extended()
                                        {
                                            self.kf.set_extended(true);
                                            if !est.within_3sigma() {
                                                warn!(
                                                    "EKF enabled @ {} but filter DIVERGING",
                                                    dt.as_gregorian_tai_str()
                                                );
                                            } else {
                                                info!(
                                                    "EKF enabled @ {}",
                                                    dt.as_gregorian_tai_str()
                                                );
                                            }
                                        }
                                        if self.kf.is_extended() {
                                            self.prop.dynamics.set_estimated_state(
                                                self.prop
                                                    .dynamics
                                                    .extract_estimated_state(&nominal_state)
                                                    + est.state_deviation(),
                                            );
                                        }
                                        self.estimates.push(est);
                                        self.residuals.push(res);
                                    }
                                    Err(e) => return Some(e),
                                }

                                // If we do not have simultaneous measurements from different devices
                                // then we don't need to check the visibility from other devices
                                // if one is in visibility.
                                if !self.simultaneous_msr {
                                    break;
                                }
                            }
                        }
                    }

                    let msr_prct = (10.0 * (msr_cnt as f64) / (num_msrs as f64)) as usize;
                    if !reported[msr_prct] {
                        info!(
                            "{:>3}% done ({:.0} measurements processed)",
                            10 * msr_prct,
                            msr_cnt
                        );
                        reported[msr_prct] = true;
                    }
                }
            }

            // Update the prev_dt for the next pass
            prev_dt = msr.epoch();
        }

        // Always report the 100% mark
        if !reported[10] {
            info!("{:>3}% done ({:.0} measurements processed)", 100, num_msrs);
        }

        None
    }

    /// Allows for covariance mapping without processing measurements
    pub fn map_covar(&mut self, end_epoch: Epoch) -> Option<FilterError> {
        let (tx, rx) = channel();
        self.prop.tx_chan = Some(tx);
        // Start by propagating the estimator (on the same thread).
        let prop_time = end_epoch - self.kf.previous_estimate().epoch();
        info!("Propagating for {} seconds", prop_time);

        self.prop.until_time_elapsed(prop_time);
        info!("Mapping covariance");

        while let Ok(nominal_state) = rx.try_recv() {
            // Update the STM of the KF (needed between each measurement or time update)
            let stm = self.prop.dynamics.extract_stm(&nominal_state);
            self.kf.update_stm(stm);
            info!("final time update {:?}", nominal_state.epoch());
            match self.kf.time_update(nominal_state) {
                Ok(est) => {
                    if self.kf.is_extended() {
                        let est_state = est.state_deviation().clone();
                        self.prop.dynamics.set_estimated_state(
                            self.prop.dynamics.extract_estimated_state(&nominal_state) + est_state,
                        );
                    }
                    self.estimates.push(est);
                }
                Err(e) => return Some(e),
            }
        }

        None
    }
}

impl<
        'a,
        D: Estimable<MsrIn, LinStateSize = Msr::StateSize>,
        E: ErrorCtrl,
        Msr: Measurement,
        N: MeasurementDevice<MsrIn, Msr>,
        A: DimName,
        K: Filter<D::LinStateSize, A, Msr::MeasurementSize, D::StateType>,
        MsrIn,
    > ODProcess<'a, D, E, Msr, N, CkfTrigger, A, K, MsrIn>
where
    D::StateType: EstimableState<Msr::StateSize>,
    DefaultAllocator: Allocator<f64, D::StateSize>
        + Allocator<f64, Msr::MeasurementSize>
        + Allocator<f64, Msr::MeasurementSize, Msr::StateSize>
        + Allocator<f64, Msr::StateSize>
        + Allocator<f64, Msr::MeasurementSize, Msr::MeasurementSize>
        + Allocator<f64, Msr::MeasurementSize, D::LinStateSize>
        + Allocator<f64, D::LinStateSize, Msr::MeasurementSize>
        + Allocator<f64, D::LinStateSize, D::LinStateSize>
        + Allocator<f64, A>
        + Allocator<f64, A, A>
        + Allocator<f64, D::LinStateSize, A>
        + Allocator<f64, A, D::LinStateSize>,
{
    pub fn ckf(
        prop: Propagator<'a, D, E>,
        kf: K,
        devices: Vec<N>,
        simultaneous_msr: bool,
        num_expected_msr: usize,
    ) -> Self {
        let mut estimates = Vec::with_capacity(num_expected_msr + 1);
        estimates.push(kf.previous_estimate().clone());
        Self {
            prop,
            kf,
            devices,
            simultaneous_msr,
            estimates,
            residuals: Vec::with_capacity(num_expected_msr),
            ekf_trigger: CkfTrigger {},
            _marker: PhantomData::<A>,
        }
    }

    pub fn default_ckf(prop: Propagator<'a, D, E>, kf: K, devices: Vec<N>) -> Self {
        let mut estimates = Vec::with_capacity(10_001);
        estimates.push(kf.previous_estimate().clone());
        Self {
            prop,
            kf,
            devices,
            simultaneous_msr: false,
            estimates,
            residuals: Vec::with_capacity(10_000),
            ekf_trigger: CkfTrigger {},
            _marker: PhantomData::<A>,
        }
    }
}
/// A trait detailing when to switch to from a CKF to an EKF
pub trait EkfTrigger {
    fn enable_ekf<S, E, T: EstimableState<S>>(&mut self, est: &E) -> bool
    where
        S: DimName,
        E: Estimate<S, T>,
        DefaultAllocator: Allocator<f64, S> + Allocator<f64, S, S>;

    /// Return true if the filter should not longer be as extended.
    /// By default, this returns false, i.e. when a filter has been switched to an EKF, it will
    /// remain as such.
    fn disable_ekf(&mut self, _epoch: Epoch) -> bool {
        false
    }
}

/// CkfTrigger will never switch a KF to an EKF
pub struct CkfTrigger;

impl EkfTrigger for CkfTrigger {
    fn enable_ekf<S, E, T: EstimableState<S>>(&mut self, _est: &E) -> bool
    where
        S: DimName,
        E: Estimate<S, T>,
        DefaultAllocator: Allocator<f64, S> + Allocator<f64, S, S>,
    {
        false
    }
}

/// An EkfTrigger on the number of measurements processed and a time between measurements.
pub struct StdEkfTrigger {
    pub num_msrs: usize,
    /// In seconds!
    pub disable_time: f64,
    /// Set to the sigma number needed to switch to the EKF (cf. 68–95–99.7 rule). If number is negative, this is ignored.
    pub within_sigma: f64,
    prev_msr_dt: Option<Epoch>,
    cur_msrs: usize,
}

impl StdEkfTrigger {
    pub fn new(num_msrs: usize, disable_time: f64) -> Self {
        Self {
            num_msrs,
            disable_time,
            within_sigma: -1.0,
            prev_msr_dt: None,
            cur_msrs: 0,
        }
    }
}

impl EkfTrigger for StdEkfTrigger {
    fn enable_ekf<S, E, T: EstimableState<S>>(&mut self, est: &E) -> bool
    where
        S: DimName,
        E: Estimate<S, T>,
        DefaultAllocator: Allocator<f64, S> + Allocator<f64, S, S>,
    {
        if !est.predicted() {
            // If this isn't a prediction, let's update the previous measurement time
            self.prev_msr_dt = Some(est.epoch());
        }
        self.cur_msrs += 1;
        self.cur_msrs >= self.num_msrs
            && ((self.within_sigma > 0.0 && est.within_sigma(self.within_sigma))
                || self.within_sigma <= 0.0)
    }

    fn disable_ekf(&mut self, epoch: Epoch) -> bool {
        // Return true if there is a prev msr dt, and the next measurement time is more than the disable time seconds away
        match self.prev_msr_dt {
            Some(prev_dt) => {
                if (epoch - prev_dt).abs() > self.disable_time {
                    self.cur_msrs = 0;
                    true
                } else {
                    false
                }
            }
            None => false,
        }
    }
}
