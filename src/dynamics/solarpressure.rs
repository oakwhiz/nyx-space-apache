use super::hyperdual::{hyperspace_from_vector, linalg::norm, Hyperdual};
use super::ForceModel;
use crate::celestia::eclipse::{EclipseLocator, EclipseState};
use crate::celestia::{Cosm, Frame, LTCorr, SpacecraftState, AU, SPEED_OF_LIGHT};
use crate::dimensions::{DimName, Matrix3, Vector3, U3, U7};
use crate::errors::NyxError;
use std::sync::Arc;

/// Computation of solar radiation pressure is based on STK: http://help.agi.com/stk/index.htm#gator/eq-solar.htm .
#[derive(Clone)]
pub struct SolarPressure {
    /// in m^2
    pub sc_area: f64,
    /// coefficient of reflectivity, must be between 0.0 (translucent) and 2.0 (all radiation absorbed and twice the force is transmitted back).
    pub cr: f64,
    /// solar flux at 1 AU, in W/m^2
    pub phi: f64,
    pub e_loc: EclipseLocator,
}

impl<'a> SolarPressure {
    /// Will use Cr = 1.8, Phi = 1367.0
    pub fn default_raw(sc_area: f64, shadow_bodies: Vec<Frame>, cosm: Arc<Cosm>) -> Self {
        let e_loc = EclipseLocator {
            light_source: cosm.frame("Sun J2000"),
            shadow_bodies,
            cosm,
            correction: LTCorr::None,
        };
        Self {
            sc_area,
            cr: 1.8,
            phi: 1367.0,
            e_loc,
        }
    }

    pub fn default(sc_area: f64, shadow_bodies: Vec<Frame>, cosm: Arc<Cosm>) -> Arc<Self> {
        Arc::new(Self::default_raw(sc_area, shadow_bodies, cosm))
    }
}

impl ForceModel for SolarPressure {
    fn eom(&self, ctx: &SpacecraftState) -> Result<Vector3<f64>, NyxError> {
        let osc = &ctx.orbit;
        // Compute the position of the Sun as seen from the spacecraft
        let r_sun = self
            .e_loc
            .cosm
            .frame_chg(osc, self.e_loc.light_source)
            .radius();
        let r_sun_unit = r_sun / r_sun.norm();

        // Compute the shaddowing factor.
        let k = match self.e_loc.compute(osc) {
            EclipseState::Umbra => 0.0,
            EclipseState::Visibilis => 1.0,
            EclipseState::Penumbra(val) => val,
        };

        let r_sun_au = r_sun.norm() / AU;
        // in N/(m^2)
        let flux_pressure = (k * self.phi / SPEED_OF_LIGHT) * (1.0 / r_sun_au).powi(2);

        // Note the 1e-3 is to convert the SRP from m/s^2 to km/s^2
        Ok(-1e-3 * self.cr * self.sc_area * flux_pressure * r_sun_unit)
    }

    fn dual_eom(
        &self,
        _radius: &Vector3<Hyperdual<f64, U7>>,
        ctx: &SpacecraftState,
    ) -> Result<(Vector3<f64>, Matrix3<f64>), NyxError> {
        let osc = ctx.orbit;

        // Compute the position of the Sun as seen from the spacecraft
        let r_sun = self
            .e_loc
            .cosm
            .frame_chg(&osc, self.e_loc.light_source)
            .radius();

        let r_sun_d: Vector3<Hyperdual<f64, U7>> = hyperspace_from_vector(&r_sun);
        let r_sun_unit = r_sun_d / norm(&r_sun_d);

        // Compute the shaddowing factor.
        let k = match self.e_loc.compute(&osc) {
            EclipseState::Umbra => 0.0,
            EclipseState::Visibilis => 1.0,
            EclipseState::Penumbra(val) => val,
        };

        let inv_r_sun_au = Hyperdual::<f64, U7>::from_real(1.0) / (norm(&r_sun_d) / AU);
        let inv_r_sun_au_p2 = inv_r_sun_au * inv_r_sun_au;
        // in N/(m^2)
        let flux_pressure =
            Hyperdual::<f64, U7>::from_real(k * self.phi / SPEED_OF_LIGHT) * inv_r_sun_au_p2;

        // Note the 1e-3 is to convert the SRP from m/s^2 to km/s^2
        let dual_force_scalar =
            Hyperdual::<f64, U7>::from_real(-1e-3 * self.cr * self.sc_area) * flux_pressure;
        let mut dual_force: Vector3<Hyperdual<f64, U7>> = Vector3::zeros();
        dual_force[0] = dual_force_scalar * r_sun_unit[0];
        dual_force[1] = dual_force_scalar * r_sun_unit[1];
        dual_force[2] = dual_force_scalar * r_sun_unit[2];

        // Extract result into Vector6 and Matrix6
        let mut fx = Vector3::zeros();
        let mut grad = Matrix3::zeros();
        for i in 0..U3::dim() {
            fx[i] += dual_force[i][0];
            // NOTE: Although the hyperdual state is of size 7, we're only setting the values up to 3 (Matrix3)
            for j in 0..U3::dim() {
                grad[(i, j)] += dual_force[i][j + 1];
            }
        }

        Ok((fx, grad))
    }
}
