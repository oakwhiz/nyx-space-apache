# nyx
[Nyx](https://en.wikipedia.org/wiki/Nyx) is a high fidelity, fast, reliable and **[validated](./VALIDATION.md)** astrodynamical toolkit library written in Rust.

The target audience is researchers and astrodynamics engineers. The rationale for using Rust is to allow for very fast computations, guaranteed thread safety,
and portability to all platforms supported by [Rust](https://forge.rust-lang.org/platform-support.html).

[![nyx-space on crates.io][cratesio-image]][cratesio]
[![nyx-space on docs.rs][docsrs-image]][docsrs]

[cratesio-image]: https://img.shields.io/crates/v/nyx-space.svg
[cratesio]: https://crates.io/crates/nyx-space
[docsrs-image]: https://docs.rs/nyx-space/badge.svg
[docsrs]: https://docs.rs/nyx-space/

# License
The [LICENSE](./LICENSE) will be strictly enforced once this toolkit reaches production-level quality.

# Features
Unless specified otherwise in the documentation of specific functions, all vectors and matrices are [statically allocated](https://discourse.nphysics.org/t/statically-typed-matrices-whose-size-is-a-multiple-or-another-one/460/4).

Lots of features are still being worked on, and there currently isn't any guarantee that the API won't change _between_ versions. However, you can be assured that the API will not change for previous versions.
Outstanding mission design features available [here](https://gitlab.com/chrisrabotin/nyx/-/issues?label_name=subsys%3A%3AMD), and orbit determination features [here](https://gitlab.com/chrisrabotin/nyx/-/issues?scope=all&utf8=%E2%9C%93&state=opened&label_name[]=subsys%3A%3AOD).

## Propagation
- [x] Propagation with different Runge Kutta methods (validated in GMAT)
- [x] Convenient and explicit definition of the dynamics for a simulation (cf. [tests/orbitaldyn.rs](tests/orbitaldyn.rs))
- [x] Propagation to different stopping conditions
- [ ] Detect orbital events in other frames ([#107](https://gitlab.com/chrisrabotin/nyx/issues/107))
## Dynamical models
- [x] Multibody dynamics using XB files (caveat: [#61](https://gitlab.com/chrisrabotin/nyx/issues/61)) (cf. [tests/orbitaldyn.rs](tests/orbitaldyn.rs))
- [x] Finite burns with fuel depletion (including low thrust / ion propulsion) (cf. [tests/prop/](tests/prop/))
- [x] Sub-Optimal Control of continuous thrust (e.g. Ruggerio, Petropoulos/Q-law) (cf. [tests/prop/closedloop_multi_oe_ruggiero.rs](tests/prop/closedloop_multi_oe_ruggiero.rs))
- [x] Solar radiation pressure modeling (cf. [tests/srp.rs](tests/srp.rs))
- [x] Basic drag models (cannonball)
- [x] Spherical harmonics ([#28](https://gitlab.com/chrisrabotin/nyx/issues/28))
- [ ] Spacecraft attitude control and some useful optimal control algorithms
## Orbit determination
- [x] Statistical Orbit Determination: Classical and Extended Kalman Filter (cf. [tests/stat_od/two_body.rs](tests/stat_od/two_body.rs))
- [x] Orbit Determination with multibody dynamics (cf. [tests/stat_od/multi_body.rs](tests/stat_od/multi_body.rs))
- [x] Smoothing and iterations of CKFs ([#105](https://gitlab.com/chrisrabotin/nyx/issues/105))
- [x] Square Root Information Filer (SRIF) ([#91](https://gitlab.com/chrisrabotin/nyx/issues/91))
- [x] An easy-to-use OD user interface ([#109](https://gitlab.com/chrisrabotin/nyx/issues/109))
- [x] Estimation with spherical harmonics enabled ([#123](https://gitlab.com/chrisrabotin/nyx/issues/123))
- [ ] Solar radiation pressure (SRP) parameter estimation ([#98](https://gitlab.com/chrisrabotin/nyx/issues/98))
- [x] Covariance mapping and estimate frame transformations ([#106](https://gitlab.com/chrisrabotin/nyx/issues/106), [#112](https://gitlab.com/chrisrabotin/nyx/issues/112))
- [x] State noise compensation (SNC) ([#85](https://gitlab.com/chrisrabotin/nyx/issues/85))
- [ ] Dynamic model compensation (DMC) ([#86](https://gitlab.com/chrisrabotin/nyx/issues/86))
- [x] High fidelity ground station placement ([#92](https://gitlab.com/chrisrabotin/nyx/issues/92))
## Celestial computations
- [x] Orbital state manipulation (from GMAT source code and validated in GMAT) (cf. [tests/state.rs](tests/state.rs))
- [x] Planetary and Solar eclipse and visibility computation (cf. [tests/eclipse.rs](tests/eclipse.rs))
- [x] Light-time corrections and abberations ([#88](https://gitlab.com/chrisrabotin/nyx/issues/88))
- [x] Frame rotations [#93](https://gitlab.com/chrisrabotin/nyx/issues/93)

# Who am I?
An astrodynamics engineer with a heavy background in software. Nyx relies on the drawbacks of
[smd](https://github.com/ChristopherRabotin/smd), a library I wrote in Go while researching at the University
of Colorado at Boulder. I work for Advanced Space ([we do really cool stuff](http://advanced-space.com/)).

# Examples
Refer to the tests for short examples.