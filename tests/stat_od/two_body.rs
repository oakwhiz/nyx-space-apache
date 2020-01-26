extern crate csv;
extern crate hifitime;
extern crate nalgebra as na;
extern crate nyx_space as nyx;

use self::hifitime::{Epoch, SECONDS_PER_DAY};
use self::na::{Matrix2, Matrix2x6, Matrix6, Vector2, Vector6, U6};
use self::nyx::celestia::{bodies, Cosm, Geoid, State};
use self::nyx::dynamics::celestial::{CelestialDynamics, CelestialDynamicsStm};
use self::nyx::dynamics::Dynamics;
use self::nyx::od::ui::*;
use self::nyx::propagators::{PropOpts, Propagator, RK4Fixed};
use std::f64::EPSILON;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

macro_rules! f64_nil {
    ($x:expr, $msg:expr) => {
        assert!($x.abs() < EPSILON, $msg)
    };
}

#[test]
fn empty_estimate() {
    let empty = Estimate::<U6>::zeros();
    f64_nil!(empty.state.norm(), "expected state norm to be nil");
    f64_nil!(empty.covar.norm(), "expected covar norm to be nil");
    f64_nil!(empty.stm.norm(), "expected STM norm to be nil");
    assert_eq!(empty.predicted, true, "expected predicted to be true");
}

#[test]
fn csv_serialize_empty_estimate() {
    use std::io;
    let empty = Estimate::<U6>::zeros();
    let mut wtr = csv::Writer::from_writer(io::stdout());
    wtr.serialize(empty).expect("could not write to stdout");
}

#[test]
fn filter_errors() {
    let initial_estimate = Estimate::<U6>::zeros();
    let measurement_noise = Matrix2::zeros();
    let real_obs = Vector2::zeros();
    let computed_obs = Vector2::zeros();
    let sensitivity = Matrix2x6::zeros();
    let stm = Matrix6::zeros();
    let dt = Epoch::from_tai_seconds(0.0);

    let mut ckf = KF::initialize(initial_estimate, measurement_noise);
    match ckf.time_update(dt) {
        Ok(_) => panic!("expected the time update to fail"),
        Err(e) => {
            assert_eq!(e, FilterError::StateTransitionMatrixNotUpdated);
        }
    }
    match ckf.measurement_update(dt, real_obs, computed_obs) {
        Ok(_) => panic!("expected the measurement update to fail"),
        Err(e) => {
            assert_eq!(e, FilterError::StateTransitionMatrixNotUpdated);
        }
    }
    ckf.update_stm(stm);
    match ckf.measurement_update(dt, real_obs, computed_obs) {
        Ok(_) => panic!("expected the measurement update to fail"),
        Err(e) => {
            assert_eq!(e, FilterError::SensitivityNotUpdated);
        }
    }
    ckf.update_h_tilde(sensitivity);
    match ckf.measurement_update(dt, real_obs, computed_obs) {
        Ok(_) => panic!("expected the measurement update to fail"),
        Err(e) => {
            assert_eq!(e, FilterError::GainSingular);
        }
    }
}

#[test]
fn ekf_fixed_step_perfect_stations() {
    use std::thread;

    // Define the ground stations.
    let num_meas_for_ekf = 15;
    let elevation_mask = 0.0;
    let range_noise = 0.0;
    let range_rate_noise = 0.0;
    let dss65_madrid = GroundStation::dss65_madrid(elevation_mask, range_noise, range_rate_noise);
    let dss34_canberra =
        GroundStation::dss34_canberra(elevation_mask, range_noise, range_rate_noise);
    let dss13_goldstone =
        GroundStation::dss13_goldstone(elevation_mask, range_noise, range_rate_noise);
    let all_stations = vec![dss65_madrid, dss34_canberra, dss13_goldstone];

    // Define the propagator information.
    let prop_time = SECONDS_PER_DAY;
    let step_size = 10.0;
    let opts = PropOpts::with_fixed_step(step_size);

    // Define the storages (channels for the states and a map for the measurements).
    let (truth_tx, truth_rx): (Sender<State<Geoid>>, Receiver<State<Geoid>>) = mpsc::channel();
    let mut measurements = Vec::with_capacity(10000); // Assume that we won't get more than 10k measurements.

    // Define state information.
    let cosm = Cosm::from_xb("./de438s");
    let earth_geoid = cosm.geoid_from_id(bodies::EARTH);
    let dt = Epoch::from_mjd_tai(21545.0);
    let initial_state =
        State::from_keplerian(22000.0, 0.01, 30.0, 80.0, 40.0, 0.0, dt, earth_geoid);

    // Generate the truth data on one thread.
    thread::spawn(move || {
        let mut dynamics = CelestialDynamics::two_body(initial_state);
        let mut prop = Propagator::new::<RK4Fixed>(&mut dynamics, &opts);
        prop.tx_chan = Some(&truth_tx);
        prop.until_time_elapsed(prop_time);
    });

    // Receive the states on the main thread, and populate the measurement channel.
    let mut prev_t = dt;
    while let Ok(rx_state) = truth_rx.recv() {
        // Convert the state to ECI.
        for station in all_stations.iter() {
            let meas = station.measure(&rx_state);
            if meas.visible() {
                let delta = rx_state.dt.as_tai_seconds() - prev_t.as_tai_seconds();
                measurements.push((delta, meas));
                prev_t = rx_state.dt;
                break; // We know that only one station is in visibility at each time.
            }
        }
    }

    // Now that we have the truth data, let's start an OD with no noise at all and compute the estimates.
    // We expect the estimated orbit to be perfect since we're using strictly the same dynamics, no noise on
    // the measurements, and the same time step.
    let opts_est = PropOpts::with_fixed_step(step_size);
    let mut tb_estimator = CelestialDynamicsStm::two_body(initial_state);
    let mut prop_est = Propagator::new::<RK4Fixed>(&mut tb_estimator, &opts_est);
    let covar_radius = 1.0e-6;
    let covar_velocity = 1.0e-6;
    let init_covar = Matrix6::from_diagonal(&Vector6::new(
        covar_radius,
        covar_radius,
        covar_radius,
        covar_velocity,
        covar_velocity,
        covar_velocity,
    ));

    let initial_estimate = Estimate::from_covar(dt, init_covar);

    let measurement_noise = Matrix2::from_diagonal(&Vector2::new(1e-6, 1e-3));
    let mut kf = KF::initialize(initial_estimate, measurement_noise);

    println!("Will process {} measurements", measurements.len());

    let mut this_dt = dt;

    let mut last_est = None;

    for (meas_no, (duration, real_meas)) in measurements.iter().enumerate() {
        // Propagate the dynamics to the measurement, and then start the filter.
        let delta_time = (*duration) as f64;
        prop_est.until_time_elapsed(delta_time);
        if meas_no > num_meas_for_ekf && !kf.ekf {
            println!("switched to EKF");
            kf.ekf = true;
        }
        // Update the STM of the KF
        kf.update_stm(prop_est.dynamics.stm);
        // Get the computed observation
        assert!(delta_time > 0.0, "repeated time");
        this_dt.mut_add_secs(delta_time);
        let rx_state = State::<Geoid>::from_cartesian_vec(
            &prop_est.dynamics.state.to_cartesian_vec(),
            this_dt,
            earth_geoid,
        );
        let mut still_empty = true;
        for station in all_stations.iter() {
            let computed_meas = station.measure(&rx_state);
            if computed_meas.visible() {
                kf.update_h_tilde(computed_meas.sensitivity());
                let (est, res) = kf
                    .measurement_update(
                        this_dt,
                        real_meas.observation(),
                        computed_meas.observation(),
                    )
                    .expect("wut?");
                still_empty = false;
                assert_eq!(est.predicted, false, "estimate should not be a prediction");
                assert!(
                    est.state.norm() < EPSILON,
                    "estimate error should be zero (perfect dynamics)"
                );
                assert!(
                    res.postfit.norm() < EPSILON,
                    "postfit should be zero (perfect dynamics) ({:e})",
                    res
                );
                // It's an EKF, so let's update the state in the dynamics.
                //let now = prop_est.time(); // Needed because we can't do a mutable borrow while doing an immutable one too.
                //let new_state = prop_est.dynamics.state.to_cartesian_vec() + est.state;
                //prop_est.dynamics.set_orbital_state(now, &new_state);

                // NOTE: The following does not work becayse state)vector returns everything with
                // the STM, and est.state is only the estimate state (e.g. 42 vs 6)
                prop_est.dynamics.set_state(
                    prop_est.time(),
                    &(prop_est.dynamics.state_vector() + est.state),
                );

                last_est = Some(est);
                break;
            }
        }
        if still_empty {
            // We're doing perfect everything, so we should always be in visibility
            panic!("T {} : not in visibility", this_dt.as_mjd_tai_days());
        }
    }

    // Check that the covariance deflated
    if let Some(est) = last_est {
        for i in 0..6 {
            if i < 3 {
                assert!(
                    est.covar[(i, i)].abs() < covar_radius,
                    "covar radius did not decrease"
                );
            } else {
                assert!(
                    est.covar[(i, i)].abs() < covar_velocity,
                    "covar velocity did not decrease"
                );
            }
        }
    }
}

#[test]
fn ckf_fixed_step_perfect_stations() {
    use std::{io, thread};

    // Define the ground stations.
    let elevation_mask = 0.0;
    let range_noise = 0.0;
    let range_rate_noise = 0.0;
    let dss65_madrid = GroundStation::dss65_madrid(elevation_mask, range_noise, range_rate_noise);
    let dss34_canberra =
        GroundStation::dss34_canberra(elevation_mask, range_noise, range_rate_noise);
    let dss13_goldstone =
        GroundStation::dss13_goldstone(elevation_mask, range_noise, range_rate_noise);
    let all_stations = vec![dss65_madrid, dss34_canberra, dss13_goldstone];

    // Define the propagator information.
    let prop_time = SECONDS_PER_DAY;
    let step_size = 10.0;
    let opts = PropOpts::with_fixed_step(step_size);

    // Define the storages (channels for the states and a map for the measurements).
    let (truth_tx, truth_rx): (Sender<State<Geoid>>, Receiver<State<Geoid>>) = mpsc::channel();
    let mut measurements = Vec::with_capacity(10000); // Assume that we won't get more than 10k measurements.

    // Define state information.
    let cosm = Cosm::from_xb("./de438s");
    let earth_geoid = cosm.geoid_from_id(bodies::EARTH);
    let dt = Epoch::from_mjd_tai(21545.0);
    let initial_state =
        State::from_keplerian(22000.0, 0.01, 30.0, 80.0, 40.0, 0.0, dt, earth_geoid);

    // Generate the truth data on one thread.
    thread::spawn(move || {
        let mut dynamics = CelestialDynamics::two_body(initial_state);
        let mut prop = Propagator::new::<RK4Fixed>(&mut dynamics, &opts);
        prop.tx_chan = Some(&truth_tx);
        prop.until_time_elapsed(prop_time);
    });

    // Receive the states on the main thread, and populate the measurement channel.
    while let Ok(rx_state) = truth_rx.recv() {
        // Convert the state to ECI.
        for station in all_stations.iter() {
            let meas = station.measure(&rx_state);
            if meas.visible() {
                measurements.push((rx_state.dt, meas));
                break; // We know that only one station is in visibility at each time.
            }
        }
    }

    // Now that we have the truth data, let's start an OD with no noise at all and compute the estimates.
    // We expect the estimated orbit to be perfect since we're using strictly the same dynamics, no noise on
    // the measurements, and the same time step.
    let opts_est = PropOpts::with_fixed_step(step_size);
    let mut tb_estimator = CelestialDynamicsStm::two_body(initial_state);
    let mut prop_est = Propagator::new::<RK4Fixed>(&mut tb_estimator, &opts_est);
    let covar_radius = 1.0e-3;
    let covar_velocity = 1.0e-6;
    let init_covar = Matrix6::from_diagonal(&Vector6::new(
        covar_radius,
        covar_radius,
        covar_radius,
        covar_velocity,
        covar_velocity,
        covar_velocity,
    ));

    let initial_estimate = Estimate::from_covar(dt, init_covar);

    let measurement_noise = Matrix2::from_diagonal(&Vector2::new(1e-6, 1e-3));
    let mut ckf = KF::initialize(initial_estimate, measurement_noise);

    let mut odp = ODProcess::new(
        &mut prop_est,
        &mut ckf,
        &all_stations,
        false,
        measurements.len(),
    );

    let rtn = odp.process_measurements(&measurements);
    assert!(rtn.is_none(), "kf failed");

    let mut wtr = csv::Writer::from_writer(io::stdout());
    let mut printed = false;
    for (no, est) in odp.estimates.iter().enumerate() {
        assert_eq!(
            est.predicted, false,
            "estimate {} should not be a prediction",
            no
        );
        assert!(
            est.state.norm() < 1e-6,
            "estimate error should be zero (perfect dynamics) ({:e})",
            est.state.norm()
        );

        let res = &odp.residuals[no];
        assert!(
            res.postfit.norm() < 1e-12,
            "postfit should be zero (perfect dynamics) ({:e})",
            res
        );

        if !printed {
            wtr.serialize(est.clone())
                .expect("could not write to stdout");
            printed = true;
        }
    }

    // Check that the covariance deflated
    let estimates = odp.estimates.clone();
    let est = &estimates[estimates.len() - 1];
    for i in 0..6 {
        if i < 3 {
            assert!(
                est.covar[(i, i)].abs() < covar_radius,
                "covar radius did not decrease"
            );
        } else {
            assert!(
                est.covar[(i, i)].abs() < covar_velocity,
                "covar velocity did not decrease"
            );
        }
    }

    println!("N-1 not smoothed: \n{}", estimates[estimates.len() - 2]);

    // Smooth
    if !odp.smooth().is_none() {
        panic!("smoothing failed");
    }
    let smoothed_estimates = odp.estimates.clone();
    println!(
        "N-1 smoothed: \n{}",
        smoothed_estimates[estimates.len() - 2]
    );
    let init_smoothed = smoothed_estimates[0].clone();

    // Re-initialize the propagator and the filter for iteration
    let mut tb_estimator = CelestialDynamicsStm::two_body(initial_state);
    let mut prop_est = Propagator::new::<RK4Fixed>(&mut tb_estimator, &opts_est);
    // Get the propagator to catch up to the first estimate
    prop_est.until_time_elapsed(init_smoothed.dt - initial_state.dt);
    let mut ckf = KF::initialize(init_smoothed, measurement_noise);

    let mut odp2 = ODProcess {
        prop: &mut prop_est,
        kf: &mut ckf,
        devices: &all_stations,
        simultaneous_msr: false,
        estimates: Vec::with_capacity(measurements.len()),
        residuals: Vec::with_capacity(measurements.len()),
    };

    odp2.process_measurements(&measurements);

    println!(
        "N-1 one iteration: \n{}",
        odp2.estimates[odp2.estimates.len() - 2]
    );
}
