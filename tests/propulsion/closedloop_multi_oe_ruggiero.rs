extern crate nalgebra as na;
extern crate nyx_space as nyx;

use self::nyx::celestia::{Cosm, GuidanceMode, Orbit, SpacecraftState};
use self::nyx::dynamics::thrustctrl::{Achieve, Ruggiero, Thruster};
use self::nyx::dynamics::{OrbitalDynamics, Spacecraft};
use self::nyx::propagators::events::{EventKind, EventTrackers, OrbitalEvent, SCEvent};
use self::nyx::propagators::{PropOpts, Propagator, RK4Fixed};
use self::nyx::time::{Epoch, TimeUnit};

/// NOTE: Herein shows the difference between the QLaw and Ruggiero (and other control laws).
/// The Ruggiero control law takes quite some longer to converge than the QLaw.

#[test]
fn qlaw_as_ruggiero_case_a() {
    // Source: AAS-2004-5089

    let mut cosm = Cosm::de438_raw();
    cosm.frame_mut_gm("EME2000", 398_600.433);
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let orbit = Orbit::keplerian(7000.0, 0.01, 0.05, 0.0, 0.0, 1.0, start_time, eme2k);

    let prop_time = 39.91 * TimeUnit::Day;

    // Define the dynamics
    let orbital_dyn = OrbitalDynamics::two_body();

    // Define the thruster
    let lowt = Thruster {
        thrust: 1.0,
        isp: 3100.0,
    };

    // Define the objectives
    let objectives = vec![
        Achieve::Sma {
            target: 42000.0,
            tol: 1.0,
        },
        Achieve::Ecc {
            target: 0.01,
            tol: 5e-5,
        },
    ];

    // Track these events
    let tracker = EventTrackers::from_events(vec![
        SCEvent::orbital(OrbitalEvent::new(EventKind::Sma(42000.0))),
        SCEvent::orbital(OrbitalEvent::new(EventKind::Ecc(0.01))),
    ]);

    let ruggiero_ctrl = Ruggiero::new(objectives, orbit);

    let dry_mass = 1.0;
    let fuel_mass = 299.0;

    let sc_state =
        SpacecraftState::with_thruster(orbit, dry_mass, fuel_mass, lowt, GuidanceMode::Thrust);

    let sc = Spacecraft::with_ctrl(orbital_dyn, ruggiero_ctrl);
    println!("[qlaw_as_ruggiero_case_a] {:o}", orbit);

    let setup = Propagator::new::<RK4Fixed>(
        sc.clone(),
        PropOpts::with_fixed_step(10.0 * TimeUnit::Second),
    );
    let mut prop = setup.with(sc_state);
    prop.event_trackers = tracker;
    let final_state = prop.for_duration(prop_time).unwrap();
    let fuel_usage = fuel_mass - final_state.fuel_mass_kg;
    println!("[qlaw_as_ruggiero_case_a] {:o}", final_state.orbit);
    println!("[qlaw_as_ruggiero_case_a] fuel usage: {:.3} kg", fuel_usage);
    println!("[qlaw_as_ruggiero_case_a] {}", prop.event_trackers);

    assert!(
        sc.ctrl_achieved(&final_state).unwrap(),
        "objective not achieved"
    );

    assert!((fuel_usage - 93.449).abs() < 1.0);
}

#[test]
fn qlaw_as_ruggiero_case_b() {
    // Source: AAS-2004-5089
    let cosm = Cosm::de438();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let orbit = Orbit::keplerian(24505.9, 0.725, 7.05, 0.0, 0.0, 0.0, start_time, eme2k);

    let prop_time = 160.0 * TimeUnit::Day;

    // Define the dynamics
    let orbital_dyn = OrbitalDynamics::two_body();

    // Define the thruster
    let lowt = Thruster {
        thrust: 0.350,
        isp: 2000.0,
    };

    // Define the objectives
    let objectives = vec![
        Achieve::Sma {
            target: 42165.0,
            tol: 20.0,
        },
        Achieve::Ecc {
            target: 0.001,
            tol: 5e-5,
        },
        Achieve::Inc {
            target: 0.05,
            tol: 5e-3,
        },
    ];

    let ruggiero_ctrl = Ruggiero::new(objectives, orbit);

    let fuel_mass = 1999.9;
    let dry_mass = 0.1;

    let sc_state =
        SpacecraftState::with_thruster(orbit, dry_mass, fuel_mass, lowt, GuidanceMode::Thrust);

    let sc = Spacecraft::with_ctrl(orbital_dyn, ruggiero_ctrl);
    println!("[qlaw_as_ruggiero_case_b] {:o}", orbit);

    let final_state = Propagator::new::<RK4Fixed>(
        sc.clone(),
        PropOpts::with_fixed_step(10.0 * TimeUnit::Second),
    )
    .with(sc_state)
    .for_duration(prop_time)
    .unwrap();

    let fuel_usage = fuel_mass - final_state.fuel_mass_kg;
    println!("[qlaw_as_ruggiero_case_b] {:o}", final_state.orbit);
    println!("[qlaw_as_ruggiero_case_b] fuel usage: {:.3} kg", fuel_usage);

    assert!(
        sc.ctrl_achieved(&final_state).unwrap(),
        "objective not achieved"
    );

    assert!((fuel_usage - 223.515).abs() < 1.0);
}

#[test]
fn qlaw_as_ruggiero_case_c() {
    // Source: AAS-2004-5089
    let cosm = Cosm::de438();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let orbit = Orbit::keplerian(9222.7, 0.2, 0.573, 0.0, 0.0, 0.0, start_time, eme2k);

    let prop_time = 3.0 * TimeUnit::Day;

    // Define the dynamics
    let orbital_dyn = OrbitalDynamics::two_body();

    // Define the thruster
    let lowt = Thruster {
        thrust: 9.3,
        isp: 3100.0,
    };

    // Define the objectives
    let objectives = vec![
        Achieve::Sma {
            target: 30000.0,
            tol: 1.0,
        },
        Achieve::Ecc {
            target: 0.7,
            tol: 5e-5,
        },
    ];

    let ruggiero_ctrl = Ruggiero::new(objectives, orbit);

    let fuel_mass = 299.9;
    let dry_mass = 0.1;

    let sc_state =
        SpacecraftState::with_thruster(orbit, dry_mass, fuel_mass, lowt, GuidanceMode::Thrust);

    let sc = Spacecraft::with_ctrl(orbital_dyn, ruggiero_ctrl);
    println!("[qlaw_as_ruggiero_case_c] {:o}", orbit);

    let final_state = Propagator::new::<RK4Fixed>(
        sc.clone(),
        PropOpts::with_fixed_step(10.0 * TimeUnit::Second),
    )
    .with(sc_state)
    .for_duration(prop_time)
    .unwrap();

    let fuel_usage = fuel_mass - final_state.fuel_mass_kg;
    println!("[qlaw_as_ruggiero_case_c] {:o}", final_state.orbit);
    println!("[qlaw_as_ruggiero_case_c] fuel usage: {:.3} kg", fuel_usage);

    assert!(
        sc.ctrl_achieved(&final_state).unwrap(),
        "objective not achieved"
    );
    assert!((fuel_usage - 41.742).abs() < 1.0);
}

#[test]
#[ignore]
fn qlaw_as_ruggiero_case_d() {
    // Broken: https://gitlab.com/chrisrabotin/nyx/issues/103
    // Source: AAS-2004-5089
    let cosm = Cosm::de438();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let orbit = Orbit::keplerian(24505.9, 0.725, 0.06, 0.0, 0.0, 0.0, start_time, eme2k);

    let prop_time = 113.0 * TimeUnit::Day;

    // Define the dynamics
    let orbital_dyn = OrbitalDynamics::two_body();

    // Define the thruster
    let lowt = Thruster {
        thrust: 89e-3,
        isp: 1650.0,
    };

    // Define the objectives
    let objectives = vec![
        Achieve::Sma {
            target: 26500.0,
            tol: 1.0,
        },
        Achieve::Inc {
            target: 116.0,
            tol: 5e-3,
        },
        Achieve::Ecc {
            target: 0.7,
            tol: 5e-5,
        },
        Achieve::Raan {
            target: 360.0 - 90.0,
            tol: 5e-3,
        },
    ];

    let ruggiero_ctrl = Ruggiero::new(objectives, orbit);

    let fuel_mass = 67.0;
    let dry_mass = 300.0;

    let sc_state =
        SpacecraftState::with_thruster(orbit, dry_mass, fuel_mass, lowt, GuidanceMode::Thrust);

    let sc = Spacecraft::with_ctrl(orbital_dyn, ruggiero_ctrl);
    println!("[qlaw_as_ruggiero_case_d] {:o}", orbit);

    let final_state = Propagator::new::<RK4Fixed>(
        sc.clone(),
        PropOpts::with_fixed_step(10.0 * TimeUnit::Second),
    )
    .with(sc_state)
    .for_duration(prop_time)
    .unwrap();

    let fuel_usage = fuel_mass - final_state.fuel_mass_kg;
    println!("[qlaw_as_ruggiero_case_d] {:o}", final_state.orbit);
    println!("[qlaw_as_ruggiero_case_d] fuel usage: {:.3} kg", fuel_usage);

    assert!(
        sc.ctrl_achieved(&final_state).unwrap(),
        "objective not achieved"
    );

    assert!((fuel_usage - 23.0).abs() < 1.0);
}

#[test]
#[ignore]
fn qlaw_as_ruggiero_case_e() {
    // Broken: https://gitlab.com/chrisrabotin/nyx/issues/103
    // Source: AAS-2004-5089
    let cosm = Cosm::de438();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let orbit = Orbit::keplerian(24505.9, 0.725, 0.06, 0.0, 0.0, 0.0, start_time, eme2k);

    let prop_time = 400.0 * TimeUnit::Day;

    // Define the dynamics
    let orbital_dyn = OrbitalDynamics::two_body();

    // Define the thruster
    let lowt = Thruster {
        thrust: 89e-3,
        isp: 1650.0,
    };

    // Define the objectives
    let objectives = vec![
        Achieve::Sma {
            target: 26500.0,
            tol: 1.0,
        },
        Achieve::Ecc {
            target: 0.7,
            tol: 5e-5,
        },
        Achieve::Inc {
            target: 116.0,
            tol: 5e-3,
        },
        Achieve::Raan {
            target: 270.0,
            tol: 5e-3,
        },
        Achieve::Aop {
            target: 180.0,
            tol: 5e-3,
        },
    ];

    let ruggiero_ctrl = Ruggiero::new(objectives, orbit);

    let fuel_mass = 1999.9;
    let dry_mass = 0.1;

    let sc_state =
        SpacecraftState::with_thruster(orbit, dry_mass, fuel_mass, lowt, GuidanceMode::Thrust);

    let sc = Spacecraft::with_ctrl(orbital_dyn, ruggiero_ctrl);
    println!("[qlaw_as_ruggiero_case_e] {:o}", orbit);

    let final_state = Propagator::new::<RK4Fixed>(
        sc.clone(),
        PropOpts::with_fixed_step(10.0 * TimeUnit::Second),
    )
    .with(sc_state)
    .for_duration(prop_time)
    .unwrap();

    let fuel_usage = fuel_mass - final_state.fuel_mass_kg;
    println!("[qlaw_as_ruggiero_case_e] {:o}", final_state.orbit);
    println!("[qlaw_as_ruggiero_case_e] fuel usage: {:.3} kg", fuel_usage);

    assert!(
        sc.ctrl_achieved(&final_state).unwrap(),
        "objective not achieved"
    );

    assert!((fuel_usage - 23.0).abs() < 1.0);
}

#[test]
fn qlaw_as_ruggiero_case_f() {
    // Source: AAS-2004-5089
    /*
        NOTE: Due to how lifetime of variables work in Rust, we need to define all of the
        components of a spacecraft before defining the spacecraft itself.
    */

    // We'll export this trajectory as a POC. Adding the needed crates here
    extern crate csv;
    use std::sync::mpsc;
    use std::sync::mpsc::{Receiver, Sender};
    use std::thread;

    let cosm = Cosm::de438();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let orbit = Orbit::keplerian(15378.0, 0.01, 98.7, 0.0, 0.0, 0.0, start_time, eme2k);

    let prop_time = 30.0 * TimeUnit::Day;

    // Define the dynamics
    let orbital_dyn = OrbitalDynamics::two_body();

    // Define the thruster
    let lowt = Thruster {
        thrust: 89e-3,
        isp: 1650.0,
    };

    // Define the objectives
    let objectives = vec![Achieve::Ecc {
        target: 0.15,
        tol: 1e-5,
    }];

    let ruggiero_ctrl = Ruggiero::new(objectives, orbit);

    let fuel_mass = 67.0;
    let dry_mass = 300.0;

    let sc_state =
        SpacecraftState::with_thruster(orbit, dry_mass, fuel_mass, lowt, GuidanceMode::Thrust);

    let sc = Spacecraft::with_ctrl(orbital_dyn, ruggiero_ctrl);
    println!("[qlaw_as_ruggiero_case_f] {:o}", orbit);

    let (tx, rx): (Sender<SpacecraftState>, Receiver<SpacecraftState>) = mpsc::channel();

    // Set up the writing channel
    thread::spawn(move || {
        let mut wtr = csv::Writer::from_path("./rugg_case_f.csv").expect("could not create file");
        while let Ok(rx_state) = rx.recv() {
            // Serialize only the orbital state
            wtr.serialize(rx_state.orbit)
                .expect("could not serialize state");
        }
    });

    let setup = Propagator::new::<RK4Fixed>(
        sc.clone(),
        PropOpts::with_fixed_step(10.0 * TimeUnit::Second),
    );
    let mut prop = setup.with(sc_state);
    prop.tx_chan = Some(tx);
    let final_state = prop.for_duration(prop_time).unwrap();

    let fuel_usage = fuel_mass - final_state.fuel_mass_kg;
    println!("[qlaw_as_ruggiero_case_f] {:o}", final_state.orbit);
    println!("[qlaw_as_ruggiero_case_f] fuel usage: {:.3} kg", fuel_usage);

    assert!(
        sc.ctrl_achieved(&final_state).unwrap(),
        "objective not achieved"
    );

    assert!((fuel_usage - 10.378).abs() < 1.0);
}

#[test]
fn ruggiero_iepc_2011_102() {
    // Source: IEPC 2011 102
    let cosm = Cosm::de438();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let orbit = Orbit::keplerian(24396.0, 0.7283, 7.0, 1.0, 1.0, 1.0, start_time, eme2k);

    let prop_time = 105.0 * TimeUnit::Day;

    // Define the dynamics
    let orbital_dyn = OrbitalDynamics::two_body();

    // Define the thruster
    let lowt = Thruster {
        thrust: 89e-3,
        isp: 1650.0,
    };

    // Define the objectives
    let objectives = vec![
        Achieve::Sma {
            target: 42164.0,
            tol: 20.0,
        },
        Achieve::Inc {
            target: 0.001,
            tol: 5e-3,
        },
        Achieve::Ecc {
            target: 0.001,
            tol: 5e-5,
        },
    ];

    let ruggiero_ctrl = Ruggiero::new(objectives, orbit);

    let fuel_mass = 67.0;
    let dry_mass = 300.0;

    let sc_state =
        SpacecraftState::with_thruster(orbit, dry_mass, fuel_mass, lowt, GuidanceMode::Thrust);

    let sc = Spacecraft::with_ctrl(orbital_dyn, ruggiero_ctrl);
    println!("[ruggiero_iepc_2011_102] {:o}", orbit);

    let final_state = Propagator::new::<RK4Fixed>(
        sc.clone(),
        PropOpts::with_fixed_step(10.0 * TimeUnit::Second),
    )
    .with(sc_state)
    .for_duration(prop_time)
    .unwrap();

    let fuel_usage = fuel_mass - final_state.fuel_mass_kg;
    println!("[ruggiero_iepc_2011_102] {:o}", final_state.orbit);
    println!("[ruggiero_iepc_2011_102] fuel usage: {:.3} kg", fuel_usage);

    assert!(
        sc.ctrl_achieved(&final_state).unwrap(),
        "objective not achieved"
    );

    // WARNING: Paper claims this can be done with only 49kg of fuel.
    assert!((fuel_usage - 49.0).abs() < 1.0);
}
