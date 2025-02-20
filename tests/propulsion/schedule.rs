extern crate nyx_space as nyx;

use self::nyx::celestia::{Bodies, Cosm, Frame, GuidanceMode, Orbit, SpacecraftState};
use self::nyx::dimensions::Vector3;
use self::nyx::dynamics::thrustctrl::{FiniteBurns, Mnvr, Thruster};
use self::nyx::dynamics::{OrbitalDynamics, Spacecraft};
use self::nyx::propagators::{PropOpts, Propagator};
use self::nyx::time::{Epoch, TimeUnit};
use self::nyx::utils::rss_errors;

#[test]
fn val_transfer_schedule_no_depl() {
    /*
        NOTE: Due to how lifetime of variables work in Rust, we need to define all of the
        components of a spacecraft before defining the spacecraft itself.
    */

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    // Build the initial spacecraft state
    let start_time = Epoch::from_gregorian_tai_at_midnight(2002, 1, 1);
    let orbit = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, start_time, eme2k,
    );

    // Define the thruster
    let monoprop = Thruster {
        thrust: 10.0,
        isp: 300.0,
    };
    let dry_mass = 1e3;
    let fuel_mass = 756.0;
    let sc_state = SpacecraftState::with_thruster(
        orbit,
        dry_mass,
        fuel_mass,
        monoprop,
        GuidanceMode::Custom(0),
    );

    let prop_time = 50.0 * TimeUnit::Minute;

    let end_time = start_time + prop_time;

    // Define the dynamics
    let bodies = vec![Bodies::Luna, Bodies::Sun, Bodies::JupiterBarycenter];
    let orbital_dyn = OrbitalDynamics::point_masses(orbit.frame, &bodies, cosm);

    // With 100% thrust: RSS errors:     pos = 3.14651e1 km      vel = 3.75245e-2 km/s

    // Define the maneuver and its schedule
    let mnvr0 = Mnvr {
        start: Epoch::from_gregorian_tai_at_midnight(2002, 1, 1),
        end: end_time,
        thrust_lvl: 1.0, // Full thrust
        vector: Vector3::new(1.0, 0.0, 0.0),
    };

    let schedule = FiniteBurns::from_mnvrs(vec![mnvr0], Frame::VNC);

    // And create the spacecraft with that controller
    // Disable fuel mass decrement
    let sc = Spacecraft::with_ctrl_no_decr(orbital_dyn, schedule);
    // Setup a propagator, and propagate for that duration
    // NOTE: We specify the use an RK89 to match the GMAT setup.
    let final_state = Propagator::rk89(sc, PropOpts::with_fixed_step(10.0 * TimeUnit::Second))
        .with(sc_state)
        .for_duration(prop_time)
        .unwrap();

    // Compute the errors
    let rslt = Orbit::cartesian(
        4_172.396_780_515_64f64,
        436.944_560_056_202_8,
        -6_518.328_156_815_674,
        -3.979_610_765_995_537,
        5.540_316_900_333_103,
        -2.207_082_771_390_863,
        end_time,
        eme2k,
    );

    let (err_r, err_v) = rss_errors(
        &final_state.orbit.to_cartesian_vec(),
        &rslt.to_cartesian_vec(),
    );
    println!("Absolute errors");
    let delta = final_state.orbit.to_cartesian_vec() - rslt.to_cartesian_vec();
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s",
        err_r, err_v,
    );

    assert!(
        err_r < 2e-10,
        format!("finite burn position wrong: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-13,
        format!("finite burn velocity wrong: {:.5e}", err_v)
    );

    // Ensure that there was no change in fuel mass since tank depletion was off
    assert!(
        (final_state.fuel_mass_kg - fuel_mass).abs() < std::f64::EPSILON,
        "incorrect fuel mass"
    );
}

#[test]
fn val_transfer_schedule_depl() {
    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    // Build the initial spacecraft state
    let start_time = Epoch::from_gregorian_tai_at_midnight(2002, 1, 1);
    let orbit = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, start_time, eme2k,
    );

    // Define the thruster
    let monoprop = Thruster {
        thrust: 10.0,
        isp: 300.0,
    };
    let dry_mass = 1e3;
    let fuel_mass = 756.0;
    let sc_state = SpacecraftState::with_thruster(
        orbit,
        dry_mass,
        fuel_mass,
        monoprop,
        GuidanceMode::Custom(0),
    );

    let prop_time = 50.0 * TimeUnit::Minute;

    let end_time = start_time + prop_time;

    // Define the dynamics
    let bodies = vec![Bodies::Luna, Bodies::Sun, Bodies::JupiterBarycenter];
    let orbital_dyn = OrbitalDynamics::point_masses(orbit.frame, &bodies, cosm);

    // With 100% thrust: RSS errors:     pos = 3.14651e1 km      vel = 3.75245e-2 km/s

    // Define the maneuver and its schedule
    let mnvr0 = Mnvr {
        start: Epoch::from_gregorian_tai_at_midnight(2002, 1, 1),
        end: end_time,
        thrust_lvl: 1.0, // Full thrust
        vector: Vector3::new(1.0, 0.0, 0.0),
    };

    let schedule = FiniteBurns::from_mnvrs(vec![mnvr0], Frame::VNC);

    // And create the spacecraft with that controller
    let sc = Spacecraft::with_ctrl(orbital_dyn, schedule);
    // Setup a propagator, and propagate for that duration
    // NOTE: We specify the use an RK89 to match the GMAT setup.
    let final_state = Propagator::rk89(sc, PropOpts::with_fixed_step(10.0 * TimeUnit::Second))
        .with(sc_state)
        .for_duration(prop_time)
        .unwrap();

    // Compute the errors
    let rslt = Orbit::cartesian(
        4_172.433_936_615_18,
        436.936_159_720_413,
        -6_518.368_821_953_345,
        -3.979_569_721_967_499,
        5.540_321_146_839_762,
        -2.207_146_819_283_441,
        end_time,
        eme2k,
    );

    let rslt_fuel_mass = 745.802_837_870_161;

    let (err_r, err_v) = rss_errors(
        &final_state.orbit.to_cartesian_vec(),
        &rslt.to_cartesian_vec(),
    );
    println!("Absolute errors");
    let delta = final_state.orbit.to_cartesian_vec() - rslt.to_cartesian_vec();
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s",
        err_r, err_v,
    );

    assert!(
        err_r < 2e-10,
        format!("finite burn position wrong: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-13,
        format!("finite burn velocity wrong: {:.5e}", err_v)
    );

    let delta_fuel_mass = (final_state.fuel_mass_kg - rslt_fuel_mass).abs();
    println!("Absolute fuel mass error: {:.0e} kg", delta_fuel_mass);
    assert!(delta_fuel_mass < 2e-10, "incorrect fuel mass");
}
