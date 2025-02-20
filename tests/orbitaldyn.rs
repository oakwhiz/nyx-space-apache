extern crate nyx_space as nyx;

use nyx::celestia::{assert_orbit_eq_or_abs, Bodies, Cosm, Orbit};
use nyx::dimensions::{Matrix6, Vector6, U3};
use nyx::dynamics::{Dynamics, OrbitalDynamics, PointMasses};
use nyx::propagators::error_ctrl::RSSStepPV;
use nyx::propagators::*;
use nyx::time::{Epoch, TimeUnit, J2000_OFFSET};
use nyx::utils::{rss_errors, rss_state_errors};
use nyx::TimeTagged;

#[allow(clippy::identity_op)]
#[test]
fn val_two_body_dynamics() {
    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438();
    let eme2k = cosm.frame("EME2000");

    let dt = Epoch::from_mjd_tai(J2000_OFFSET);
    let state = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, dt, eme2k,
    );

    let rslt = Orbit::cartesian(
        -5_971.194_376_797_643,
        3_945.517_912_574_178_4,
        2_864.620_957_744_429_2,
        0.049_083_101_605_507_95,
        -4.185_084_125_817_658,
        5.848_947_462_472_877,
        dt + prop_time,
        eme2k,
    );

    let dynamics = OrbitalDynamics::two_body();
    let setup = Propagator::rk89(dynamics, PropOpts::<RSSStepPV>::default());
    let mut prop = setup.with(state);
    prop.for_duration(prop_time).unwrap();
    assert_orbit_eq_or_abs(&prop.state, &rslt, 2e-9, "two body prop failed");

    println!("==> val_two_body_dynamics absolute errors");
    let delta = prop.state.to_cartesian_vec() - rslt.to_cartesian_vec();
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    // And now do the backprop by re-initializing a propagator to ensure correct step size
    prop.for_duration(-prop_time).unwrap();
    let (err_r, err_v) = rss_errors(&prop.state.to_cartesian_vec(), &state.to_cartesian_vec());
    println!("RTN:  {}\nINIT: {}", prop.state, state);
    dbg!(err_r);
    assert!(
        err_r < 1e-5,
        "two body back prop failed to return to the initial state in position"
    );
    assert!(
        err_v < 1e-8,
        "two body back prop failed to return to the initial state in velocity"
    );
    assert_eq!(prop.state.epoch(), dt);
    // Forward propagation again to confirm that we can do repeated calls
    prop.for_duration(prop_time).unwrap();
    assert_eq!(prop.state.epoch(), dt + prop_time);
    let (err_r, err_v) = rss_errors(&prop.state.to_cartesian_vec(), &rslt.to_cartesian_vec());
    assert!(
        err_r < 1e-5,
        "two body back+fwd prop failed to return to the initial state in position"
    );
    assert!(
        err_v < 1e-8,
        "two body back+fwd prop failed to return to the initial state in velocity"
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_halo_earth_moon_dynamics() {
    /*
    We validate against GMAT after switching the GMAT script to use de438s.bsp. We are using GMAT's default GM values.
    The state in `rslt` is exactly the GMAT output.
    */
    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let halo_rcvr = Orbit::cartesian(
        333_321.004_516,
        -76_134.198_887,
        -20_873.831_939,
        0.257_153_712,
        0.930_284_066,
        0.346_177,
        start_time,
        eme2k,
    );

    // GMAT data
    let rslt = Orbit::cartesian(
        345_395.216_758_754_4,
        5_967.890_264_751_025,
        7_350.734_617_702_599,
        0.022_370_754_768_832_33,
        0.957_450_818_399_485_1,
        0.303_172_019_604_272_5,
        start_time + prop_time,
        eme2k,
    );

    let bodies = vec![Bodies::Luna];
    let dynamics = OrbitalDynamics::point_masses(eme2k, &bodies, cosm);

    let setup = Propagator::rk89(dynamics, PropOpts::with_fixed_step(10 * TimeUnit::Second));
    let mut prop = setup.with(halo_rcvr);
    prop.for_duration(prop_time).unwrap();
    let (err_r, err_v) = rss_state_errors(&prop.state, &rslt);

    println!("==> val_halo_earth_moon_dynamics absolute errors");
    let delta = prop.state.to_cartesian_vec() - rslt.to_cartesian_vec();
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s\ninit\t{}\nfinal\t{}",
        err_r, err_v, halo_rcvr, prop.state
    );

    assert!(
        err_r < 5e-5,
        format!("multi body failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-9,
        format!("multi body failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_halo_earth_moon_dynamics_adaptive() {
    /*
    We validate against GMAT after switching the GMAT script to use de438s.bsp. We are using GMAT's default GM values.
    The state in `rslt` is exactly the GMAT output.
    */
    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2002, 2, 7);

    let halo_rcvr = Orbit::cartesian(
        333_321.004_516,
        -76_134.198_887,
        -20_873.831_939,
        0.257_153_712,
        0.930_284_066,
        0.346_177,
        start_time,
        eme2k,
    );

    let rslt = Vector6::new(
        343_016.028_193_306_2,
        6_118.870_782_679_712,
        9_463.253_311_291_08,
        -0.033_885_504_418_292_03,
        0.961_942_577_960_542_2,
        0.351_738_121_709_363_5,
    );

    let bodies = vec![Bodies::Luna];
    let dynamics = OrbitalDynamics::point_masses(eme2k, &bodies, cosm);

    let setup = Propagator::rk89(dynamics, PropOpts::default());
    let mut prop = setup.with(halo_rcvr);
    prop.for_duration(prop_time).unwrap();
    let (err_r, err_v) = rss_errors(&prop.state.to_cartesian_vec(), &rslt);

    println!("==> val_halo_earth_moon_dynamics_adaptive absolute errors");
    let delta = prop.state.to_cartesian_vec() - rslt;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s\ninit\t{}\nfinal\t{}",
        err_r, err_v, halo_rcvr, prop.state
    );

    assert!(
        err_r < 1e-6,
        format!("multi body failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-11,
        format!("multi body failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_llo_earth_moon_dynamics_adaptive() {
    /*
    We validate against GMAT after switching the GMAT script to use de438s.bsp. We are using GMAT's default GM values.
    The state in `rslt` is exactly the GMAT output.
    */
    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2002, 2, 7);

    let llo_xmtr = Orbit::cartesian(
        3.919_869_89e5,
        -7.493_039_70e4,
        -7.022_605_11e4,
        -6.802_604_18e-1,
        1.992_053_61,
        4.369_389_94e-1,
        start_time,
        eme2k,
    );

    // GMAT data
    let rslt = Vector6::new(
        322_883.886_835_433_2,
        97_580.280_858_158,
        -30_871.085_807_431_58,
        -0.934_039_629_727_003_5,
        1.980_106_615_205_608,
        0.472_630_895_504_854_4,
    );

    let bodies = vec![Bodies::Luna];
    let dynamics = OrbitalDynamics::point_masses(eme2k, &bodies, cosm);

    let setup = Propagator::rk89(dynamics, PropOpts::default());
    let mut prop = setup.with(llo_xmtr);
    prop.for_duration(prop_time).unwrap();
    let (err_r, err_v) = rss_errors(&prop.state.to_cartesian_vec(), &rslt);

    println!("==> val_llo_earth_moon_dynamics_adaptive absolute errors");
    let delta = prop.state.to_cartesian_vec() - rslt;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s\ninit\t{}\nfinal\t{}",
        err_r, err_v, llo_xmtr, prop.state
    );

    assert!(
        err_r < 1e-5,
        format!("multi body failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-8,
        format!("multi body failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_halo_multi_body_dynamics() {
    /*
    We validate against GMAT after switching the GMAT script to use de438s.bsp. We are using GMAT's default GM values.
    The state in `rslt` is exactly the GMAT output.
    */
    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let halo_rcvr = Orbit::cartesian(
        333_321.004_516,
        -76_134.198_887,
        -20_873.831_939,
        0.257_153_712,
        0.930_284_066,
        0.346_177,
        start_time,
        eme2k,
    );

    // GMAT data
    let rslt = Vector6::new(
        345_350.664_306_402_7,
        5_930.672_402_473_843,
        7_333.283_870_811_47,
        0.021_298_196_465_430_16,
        0.956_678_964_966_812_2,
        0.302_817_582_487_008_6,
    );

    let bodies = vec![Bodies::Luna, Bodies::Sun, Bodies::JupiterBarycenter];
    let dynamics = OrbitalDynamics::point_masses(eme2k, &bodies, cosm);

    let setup = Propagator::rk89(dynamics, PropOpts::with_fixed_step(10 * TimeUnit::Second));
    let mut prop = setup.with(halo_rcvr);
    prop.for_duration(prop_time).unwrap();
    let (err_r, err_v) = rss_errors(&prop.state.to_cartesian_vec(), &rslt);

    println!("==> val_halo_multi_body_dynamics absolute errors");
    let delta = prop.state.to_cartesian_vec() - rslt;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s\ninit\t{}\nfinal\t{}",
        err_r, err_v, halo_rcvr, prop.state
    );

    assert!(
        err_r < 5e-5,
        format!("multi body failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-9,
        format!("multi body failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_halo_multi_body_dynamics_adaptive() {
    /*
    We validate against GMAT after switching the GMAT script to use de438s.bsp. We are using GMAT's default GM values.
    The state in `rslt` is exactly the GMAT output.
    */

    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2002, 2, 7);

    let halo_rcvr = Orbit::cartesian(
        333_321.004_516,
        -76_134.198_887,
        -20_873.831_939,
        0.257_153_712,
        0.930_284_066,
        0.346_177,
        start_time,
        eme2k,
    );

    // GMAT data
    let rslt = Vector6::new(
        343_063.315_079_726_9,
        6_045.912_866_799_058,
        9_430.044_002_816_507,
        -0.032_841_040_500_475_27,
        0.960_272_613_530_677_2,
        0.350_981_431_322_089_4,
    );

    let bodies = vec![Bodies::Luna, Bodies::Sun, Bodies::JupiterBarycenter];
    let dynamics = OrbitalDynamics::point_masses(eme2k, &bodies, cosm);

    let setup = Propagator::default(dynamics);
    let mut prop = setup.with(halo_rcvr);
    prop.for_duration(prop_time).unwrap();
    let (err_r, err_v) = rss_errors(&prop.state.to_cartesian_vec(), &rslt);

    println!("==> val_halo_multi_body_dynamics_adaptive absolute errors");
    let delta = prop.state.to_cartesian_vec() - rslt;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s\ninit\t{}\nfinal\t{}",
        err_r, err_v, halo_rcvr, prop.state
    );

    assert!(
        err_r < 1e-6,
        format!("multi body failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-11,
        format!("multi body failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_llo_multi_body_dynamics_adaptive() {
    /*
    We validate against GMAT after switching the GMAT script to use de438s.bsp. We are using GMAT's default GM values.
    The state in `rslt` is exactly the GMAT output.
    */

    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2002, 2, 7);

    let llo_xmtr = Orbit::cartesian(
        3.919_869_89e5,
        -7.493_039_70e4,
        -7.022_605_11e4,
        -6.802_604_18e-1,
        1.992_053_61,
        4.369_389_94e-1,
        start_time,
        eme2k,
    );

    // GMAT data
    let rslt = Vector6::new(
        322_931.851_760_741_2,
        97_497.699_738_811_13,
        -30_899.323_820_367_2,
        -0.933_095_202_143_736_8,
        1.978_291_140_770_421,
        0.472_036_197_968_369_3,
    );

    let bodies = vec![Bodies::Luna, Bodies::Sun, Bodies::JupiterBarycenter];
    let dynamics = OrbitalDynamics::point_masses(eme2k, &bodies, cosm);

    let setup = Propagator::default(dynamics);
    let mut prop = setup.with(llo_xmtr);
    prop.for_duration(prop_time).unwrap();
    let (err_r, err_v) = rss_errors(&prop.state.to_cartesian_vec(), &rslt);

    println!("==> val_llo_multi_body_dynamics_adaptive absolute errors");
    let delta = prop.state.to_cartesian_vec() - rslt;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s\ninit\t{}\nfinal\t{}",
        err_r, err_v, llo_xmtr, prop.state
    );

    assert!(
        err_r < 2e-6,
        format!("multi body failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-9,
        format!("multi body failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_leo_multi_body_dynamics_adaptive_wo_moon() {
    /*
    We validate against GMAT after switching the GMAT script to use de438s.bsp. We are using GMAT's default GM values.
    The state in `rslt` is exactly the GMAT output.
    */

    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let leo = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, start_time, eme2k,
    );

    // GMAT data
    let rslt = Vector6::new(
        -5_971.190_141_842_914,
        3_945.572_972_028_369,
        2_864.554_642_502_679,
        0.049_014_376_371_383_95,
        -4.185_051_832_316_421,
        5.848_971_837_743_221,
    );

    let bodies = vec![Bodies::Luna, Bodies::Sun, Bodies::JupiterBarycenter];
    let dynamics = OrbitalDynamics::point_masses(eme2k, &bodies, cosm);

    let setup = Propagator::default(dynamics);
    let mut prop = setup.with(leo);
    prop.for_duration(prop_time).unwrap();
    let (err_r, err_v) = rss_errors(&prop.state.to_cartesian_vec(), &rslt);

    println!("==> val_leo_multi_body_dynamics_adaptive_wo_moon absolute errors");
    let delta = prop.state.to_cartesian_vec() - rslt;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s\ninit\t{}\nfinal\t{}",
        err_r, err_v, leo, prop.state
    );

    assert!(
        err_r < 5e-7,
        format!("multi body failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 5e-10,
        format!("multi body failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_leo_multi_body_dynamics_adaptive() {
    /*
    We validate against GMAT after switching the GMAT script to use de438s.bsp. We are using GMAT's default GM values.
    The state in `rslt` is exactly the GMAT output.
    */

    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let leo = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, start_time, eme2k,
    );

    // GMAT data
    let rslt = Vector6::new(
        -5_971.190_491_039_24,
        3_945.529_211_711_111,
        2_864.613_171_213_388,
        0.049_086_325_111_121_92,
        -4.185_065_854_096_239,
        5.848_960_991_136_447,
    );

    let bodies = vec![Bodies::Sun, Bodies::JupiterBarycenter];
    let dynamics = OrbitalDynamics::point_masses(eme2k, &bodies, cosm);

    let setup = Propagator::default(dynamics);
    let mut prop = setup.with(leo);
    prop.for_duration(prop_time).unwrap();
    let (err_r, err_v) = rss_errors(&prop.state.to_cartesian_vec(), &rslt);

    println!("==> val_leo_multi_body_dynamics_adaptive absolute errors");
    let delta = prop.state.to_cartesian_vec() - rslt;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    println!(
        "RSS errors:\tpos = {:.5e} km\tvel = {:.5e} km/s\ninit\t{}\nfinal\t{}",
        err_r, err_v, leo, prop.state
    );

    assert!(
        err_r < 3e-6,
        format!("multi body failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 3e-9,
        format!("multi body failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn two_body_dual() {
    // This is a duplicate of the differentials test in hyperdual.

    let cosm = Cosm::de438();
    let eme2k = cosm.frame("EME2000");

    let init = Orbit::cartesian_stm(
        -9_042.862_233_600_335,
        18_536.333_069_123_244,
        6_999.957_069_486_411_5,
        -3.288_789_003_770_57,
        -2.226_285_193_102_822,
        1.646_738_380_722_676_5,
        Epoch::from_mjd_tai(21_546.0),
        eme2k,
    );

    let expected_fx = Vector6::new(
        -3.288_789_003_770_57,
        -2.226_285_193_102_822,
        1.646_738_380_722_676_5,
        0.000_348_875_166_673_120_14,
        -0.000_715_134_890_031_838_4,
        -0.000_270_059_537_150_490_5,
    );

    let dynamics = OrbitalDynamics::two_body();
    let (fx, grad) = dynamics
        .eom_grad(0.0, &init.to_cartesian_vec(), &init)
        .unwrap();

    assert!(
        (fx - expected_fx).norm() < 1e-16,
        "f(x) computation is incorrect {:e}",
        (fx - expected_fx).norm()
    );

    let mut expected = Matrix6::zeros();

    expected[(0, 3)] = 1.0;
    expected[(1, 4)] = 1.0;
    expected[(2, 5)] = 1.0;
    expected[(3, 0)] = -0.000_000_018_628_398_391_083_86;
    expected[(4, 0)] = -0.000_000_040_897_747_124_379_53;
    expected[(5, 0)] = -0.000_000_015_444_396_313_003_294;
    expected[(3, 1)] = -0.000_000_040_897_747_124_379_53;
    expected[(4, 1)] = 0.000_000_045_253_271_058_430_05;
    expected[(5, 1)] = 0.000_000_031_658_391_636_846_51;
    expected[(3, 2)] = -0.000_000_015_444_396_313_003_294;
    expected[(4, 2)] = 0.000_000_031_658_391_636_846_51;
    expected[(5, 2)] = -0.000_000_026_624_872_667_346_21;

    assert!(
        (grad - expected).norm() < 1e-16,
        "gradient computation is incorrect {:e}",
        (grad - expected).norm()
    );

    let prop_time = 1 * TimeUnit::Day;

    let setup = Propagator::rk89(dynamics, PropOpts::with_fixed_step(10 * TimeUnit::Second));
    let mut prop = setup.with(init);
    let final_state = prop.for_duration(prop_time).unwrap();

    // Check that the STM is correct by back propagating by the previous step, and multiplying by the STM.
    let final_stm = final_state.stm.unwrap();
    let final_step = prop.latest_details().step;
    prop.for_duration(-final_step).unwrap();

    // And check the difference
    let stm_err = final_stm * prop.state.to_cartesian_vec() - final_state.to_cartesian_vec();
    let radius_err = stm_err.fixed_rows::<U3>(0).into_owned();
    let velocity_err = stm_err.fixed_rows::<U3>(3).into_owned();

    assert!(radius_err.norm() < 1e-1);
    assert!(velocity_err.norm() < 1e-1);
}

#[allow(clippy::identity_op)]
#[test]
fn multi_body_dynamics_dual() {
    let prop_time = 1 * TimeUnit::Day;

    let cosm = Cosm::de438();
    let eme2k = cosm.frame("EME2000");

    let start_time = Epoch::from_gregorian_tai_at_midnight(2020, 1, 1);

    let halo_rcvr = Orbit::cartesian_stm(
        333_321.004_516,
        -76_134.198_887,
        -20_873.831_939,
        0.257_153_712,
        0.930_284_066,
        0.346_177,
        start_time,
        eme2k,
    );

    let bodies = vec![Bodies::Luna, Bodies::Sun, Bodies::JupiterBarycenter];
    let dynamics = OrbitalDynamics::point_masses(eme2k, &bodies, cosm);

    let setup = Propagator::rk89(dynamics, PropOpts::with_fixed_step(10 * TimeUnit::Second));
    let mut prop = setup.with(halo_rcvr);
    let final_state = prop.for_duration(prop_time).unwrap();

    // Check that the STM is correct by back propagating by the previous step, and multiplying by the STM.
    let final_stm = final_state.stm.unwrap();
    let final_step = prop.latest_details().step;
    prop.for_duration(-final_step).unwrap();

    // And check the difference
    let stm_err = final_stm * prop.state.to_cartesian_vec() - final_state.to_cartesian_vec();
    let radius_err = stm_err.fixed_rows::<U3>(0).into_owned();
    let velocity_err = stm_err.fixed_rows::<U3>(3).into_owned();

    assert!(radius_err.norm() < 1e-3);
    assert!(velocity_err.norm() < 1e-3);
}

#[allow(clippy::identity_op)]
#[test]
fn val_earth_sph_harmonics_j2() {
    // NOTE: GMAT and Monte are within about 0.1 meters of difference in position. Hence, we're checking we're in the same bracket.
    use nyx::dynamics::Harmonics;
    use nyx::io::gravity::*;
    use std::sync::Arc;

    let monte_earth_gm = 3.986_004_328_969_392e5;
    let monte_earth_j2 = -0.000_484_169_325_971;

    let mut cosm = Cosm::de438_raw();
    cosm.frame_mut_gm("EME2000", monte_earth_gm);
    cosm.frame_mut_gm("IAU Earth", monte_earth_gm);
    let eme2k = cosm.frame("EME2000");
    let iau_earth = cosm.frame("IAU Earth");

    let earth_sph_harm = HarmonicsMem::from_j2(monte_earth_j2);
    let harmonics = Harmonics::from_stor(iau_earth, earth_sph_harm, Arc::new(cosm));

    let dt = Epoch::from_mjd_tai(J2000_OFFSET);
    let state = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, dt, eme2k,
    );
    // GMAT validation case
    // -5751.473991327555          4721.214035135832           2045.947768608806           -0.7977402618596746          -3.656451983636495           6.139637921620765
    /*
    let rslt_gmat = Vector6::new(
        -5_751.473_991_327_555,
        4721.214035135832,
        2045.947768608806,
        -0.7977402618596746,
        -3.656451983636495,
        6.139637921620765,
    );*/

    // Monte validation case
    // Orbit (km, km/sec)
    // 'Earth' -> 'test' in 'EME2000' at '02-JAN-2000 12:00:00.0000 TAI'
    // Pos: -5.751472565170783e+03  4.721183256208691e+03  2.046020865167045e+03
    // Vel: -7.976895830677169e-01 -3.656498994998706e+00  6.139616747276084e+00
    let rslt_monte = Vector6::new(
        -5.751_472_565_170_783e3,
        4.721_183_256_208_691e3,
        2.046_020_865_167_045e3,
        -7.976_895_830_677_169e-1,
        -3.656_498_994_998_706,
        6.139_616_747_276_084,
    );

    let dynamics = OrbitalDynamics::with_model(harmonics);

    let prop_state = Propagator::rk89(dynamics, PropOpts::<RSSStepPV>::default())
        .with(state)
        .for_duration(1 * TimeUnit::Day)
        .unwrap();

    println!("{}", prop_state);

    println!("==> val_earth_sph_harmonics_j2 absolute errors (MONTE)");
    let delta = prop_state.to_cartesian_vec() - rslt_monte;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    let (err_r, err_v) = rss_errors(&prop_state.to_cartesian_vec(), &rslt_monte);

    assert!(
        err_r < 1e-1,
        format!("J2 failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-4,
        format!("J2 failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_earth_sph_harmonics_12x12() {
    extern crate pretty_env_logger;
    if pretty_env_logger::try_init().is_err() {
        println!("could not init env_logger");
    }
    use nyx::dynamics::sph_harmonics::Harmonics;
    use nyx::io::gravity::*;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");
    let iau_earth = cosm.frame("IAU Earth");

    let earth_sph_harm = HarmonicsMem::from_cof("data/JGM3.cof.gz", 12, 12, true).unwrap();
    let harmonics = Harmonics::from_stor(iau_earth, earth_sph_harm, cosm);

    let dt = Epoch::from_mjd_tai(J2000_OFFSET);
    let state = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, dt, eme2k,
    );

    // GMAT validation case
    let rslt_gmat = Vector6::new(
        -5_751.935_197_673_059,
        4_719.330_857_046_409,
        2_048.776_230_999_391,
        -0.795_315_465_634_082_6,
        -3.658_346_256_468_031,
        6.138_852_391_455_04,
    );

    let dynamics = OrbitalDynamics::with_model(harmonics);

    let prop_state = Propagator::rk89(dynamics, PropOpts::with_tolerance(1e-9))
        .with(state)
        .for_duration(1 * TimeUnit::Day)
        .unwrap();

    println!("{}", prop_state);

    println!("==> val_earth_sph_harmonics_12x12 absolute errors");
    let delta = prop_state.to_cartesian_vec() - rslt_gmat;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    let (err_r, err_v) = rss_errors(&prop_state.to_cartesian_vec(), &rslt_gmat);

    assert!(
        err_r < 1e-1,
        format!("12x12 failed in position: {:.5e}", err_r)
    );
    assert!(
        err_v < 1e-4,
        format!("12x12 failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_earth_sph_harmonics_70x70() {
    extern crate pretty_env_logger;
    if pretty_env_logger::try_init().is_err() {
        println!("could not init env_logger");
    }
    use nyx::dynamics::Harmonics;
    use nyx::io::gravity::*;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");
    let iau_earth = cosm.frame("IAU Earth");

    let earth_sph_harm = HarmonicsMem::from_cof("data/JGM3.cof.gz", 70, 70, true).unwrap();
    let harmonics = Harmonics::from_stor(iau_earth, earth_sph_harm, cosm);

    let dt = Epoch::from_mjd_tai(J2000_OFFSET);
    let state = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, dt, eme2k,
    );

    // GMAT validation case
    let rslt_gmat = Vector6::new(
        -5_751.924_618_076_704,
        4_719.386_612_440_923,
        2_048.696_011_823_441,
        -0.795_383_404_365_819_8,
        -3.658_301_183_319_466,
        6.138_865_498_487_843,
    );

    let dynamics = OrbitalDynamics::with_model(harmonics);

    let prop_rslt = Propagator::default(dynamics)
        .with(state)
        .for_duration(1 * TimeUnit::Day)
        .unwrap();

    println!("{}", prop_rslt);

    println!("==> val_earth_sph_harmonics_70x70 absolute errors");
    let delta = prop_rslt.to_cartesian_vec() - rslt_gmat;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    let (err_r, err_v) = rss_errors(&prop_rslt.to_cartesian_vec(), &rslt_gmat);

    assert!(
        dbg!(err_r) < 0.2,
        format!("70x70 failed in position: {:.5e}", err_r)
    );
    assert!(
        dbg!(err_v) < 1e-3,
        format!("70x70 failed in velocity: {:.5e}", err_v)
    );
}

#[allow(clippy::identity_op)]
#[test]
fn val_earth_sph_harmonics_70x70_partials() {
    extern crate pretty_env_logger;
    if pretty_env_logger::try_init().is_err() {
        println!("could not init env_logger");
    }
    use nyx::dynamics::Harmonics;
    use nyx::io::gravity::*;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");
    let iau_earth = cosm.frame("IAU Earth");

    let earth_sph_harm = HarmonicsMem::from_cof("data/JGM3.cof.gz", 70, 70, true).unwrap();
    let harmonics = Harmonics::from_stor(iau_earth, earth_sph_harm, cosm);

    let dt = Epoch::from_mjd_tai(J2000_OFFSET);
    let state = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, dt, eme2k,
    );

    // GMAT validation case
    let rslt_gmat = Vector6::new(
        -5_751.924_618_076_704,
        4_719.386_612_440_923,
        2_048.696_011_823_441,
        -0.795_383_404_365_819_8,
        -3.658_301_183_319_466,
        6.138_865_498_487_843,
    );

    let dynamics = OrbitalDynamics::with_model(harmonics);

    let prop_rslt = Propagator::default(dynamics)
        .with(state)
        .for_duration(1 * TimeUnit::Day)
        .unwrap();

    println!("{}", prop_rslt);

    println!("==> val_earth_sph_harmonics_70x70_partials absolute errors");
    let delta = prop_rslt.to_cartesian_vec() - rslt_gmat;
    for i in 0..6 {
        print!("{:.0e}\t", delta[i].abs());
    }
    println!();

    let (err_r, err_v) = rss_errors(&prop_rslt.to_cartesian_vec(), &rslt_gmat);

    assert!(
        dbg!(err_r) < 0.2,
        format!("12x12 failed in position: {:.5e}", err_r)
    );
    assert!(
        dbg!(err_v) < 1e-3,
        format!("12x12 failed in velocity: {:.5e}", err_v)
    );
}

#[test]
fn hf_prop() {
    // Tests a high fidelity propagation over several days for performance analysis.

    extern crate pretty_env_logger;
    if pretty_env_logger::try_init().is_err() {
        println!("could not init env_logger");
    }
    use nyx::dynamics::sph_harmonics::Harmonics;
    use nyx::io::gravity::*;

    let cosm = Cosm::de438_gmat();
    let eme2k = cosm.frame("EME2000");
    let iau_earth = cosm.frame("IAU Earth");

    let earth_sph_harm = HarmonicsMem::from_cof("data/JGM3.cof.gz", 21, 21, true).unwrap();
    let harmonics = Harmonics::from_stor(iau_earth, earth_sph_harm, cosm);

    let dt = Epoch::from_mjd_tai(J2000_OFFSET);
    let state = Orbit::cartesian(
        -2436.45, -2436.45, 6891.037, 5.088_611, -5.088_611, 0.0, dt, eme2k,
    );

    let cosm = Cosm::de438();
    let bodies = vec![Bodies::Luna, Bodies::Sun, Bodies::JupiterBarycenter];
    let dynamics = OrbitalDynamics::new(vec![PointMasses::new(eme2k, &bodies, cosm), harmonics]);

    let setup = Propagator::rk89(dynamics, PropOpts::with_tolerance(1e-9));
    let rslt = setup
        .with(state)
        .for_duration(30.0 * TimeUnit::Day)
        .unwrap();

    println!("{}\n{:o}", rslt, rslt);
}
