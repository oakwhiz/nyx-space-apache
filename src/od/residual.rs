use super::serde::ser::SerializeSeq;
use super::serde::{Serialize, Serializer};
use super::EpochFormat;
use crate::dimensions::allocator::Allocator;
use crate::dimensions::{DefaultAllocator, DimName, VectorN};
use crate::hifitime::Epoch;
use std::fmt;

/// Stores an Estimate, as the result of a `time_update` or `measurement_update`.
#[derive(Debug, Clone, PartialEq)]
pub struct Residual<M>
where
    M: DimName,
    DefaultAllocator: Allocator<f64, M> + Allocator<f64, M, M>,
{
    /// Date time of this Residual
    pub dt: Epoch,
    /// The prefit residual (set to zero for EKF filters)
    pub prefit: VectorN<f64, M>,
    /// The postfit residual (set to zero for EKF filters)
    pub postfit: VectorN<f64, M>,
    /// The Epoch format upon serialization
    pub epoch_fmt: EpochFormat,
}

impl<M> Residual<M>
where
    M: DimName,
    DefaultAllocator: Allocator<f64, M> + Allocator<f64, M, M>,
{
    /// An empty estimate. This is useful if wanting to store an estimate outside the scope of a filtering loop.
    pub fn zeros() -> Self {
        Self {
            dt: Epoch::from_tai_seconds(0.0),
            prefit: VectorN::<f64, M>::zeros(),
            postfit: VectorN::<f64, M>::zeros(),
            epoch_fmt: EpochFormat::GregorianUtc,
        }
    }

    pub fn header(epoch_fmt: EpochFormat) -> Vec<String> {
        let mut hdr_v = Vec::with_capacity(2 * M::dim() + 1);
        hdr_v.push(format!("{}", epoch_fmt));
        // Serialize the prefit
        for i in 0..M::dim() {
            hdr_v.push(format!("prefit_{}", i));
        }
        // Serialize the postfit
        for i in 0..M::dim() {
            hdr_v.push(format!("postfit_{}", i));
        }
        hdr_v
    }

    pub fn default_header() -> Vec<String> {
        Self::header(EpochFormat::GregorianUtc)
    }

    pub fn new(dt: Epoch, prefit: VectorN<f64, M>, postfit: VectorN<f64, M>) -> Self {
        Self {
            dt,
            prefit,
            postfit,
            epoch_fmt: EpochFormat::GregorianUtc,
        }
    }
}

impl<M> fmt::Display for Residual<M>
where
    M: DimName,
    DefaultAllocator:
        Allocator<f64, M> + Allocator<f64, M, M> + Allocator<usize, M> + Allocator<usize, M, M>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Prefit {} Postfit {}", &self.prefit, &self.postfit)
    }
}

impl<M> fmt::LowerExp for Residual<M>
where
    M: DimName,
    DefaultAllocator:
        Allocator<f64, M> + Allocator<f64, M, M> + Allocator<usize, M> + Allocator<usize, M, M>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Prefit {:e} Postfit {:e}", &self.prefit, &self.postfit)
    }
}

impl<M> Serialize for Residual<M>
where
    M: DimName,
    DefaultAllocator:
        Allocator<f64, M> + Allocator<f64, M, M> + Allocator<usize, M> + Allocator<usize, M, M>,
{
    /// Serializes the estimate
    fn serialize<O>(&self, serializer: O) -> Result<O::Ok, O::Error>
    where
        O: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(2 * M::dim() + 1))?;
        match self.epoch_fmt {
            EpochFormat::GregorianUtc => seq.serialize_element(&self.dt.as_gregorian_utc_str())?,
            EpochFormat::GregorianTai => seq.serialize_element(&self.dt.as_gregorian_tai_str())?,
            EpochFormat::MjdTai => seq.serialize_element(&self.dt.as_mjd_tai_days())?,
            EpochFormat::MjdTt => seq.serialize_element(&self.dt.as_mjd_tt_days())?,
            EpochFormat::MjdUtc => seq.serialize_element(&self.dt.as_mjd_utc_days())?,
            EpochFormat::JdeEt => seq.serialize_element(&self.dt.as_jde_et_days())?,
            EpochFormat::JdeTai => seq.serialize_element(&self.dt.as_jde_tai_days())?,
            EpochFormat::JdeTt => seq.serialize_element(&self.dt.as_jde_tt_days())?,
            EpochFormat::JdeUtc => seq.serialize_element(&self.dt.as_jde_utc_days())?,
            EpochFormat::TaiSecs(e) => seq.serialize_element(&(self.dt.as_tai_seconds() - e))?,
            EpochFormat::TaiDays(e) => seq.serialize_element(&(self.dt.as_tai_days() - e))?,
        }
        // Serialize the prefit
        for i in 0..M::dim() {
            seq.serialize_element(&self.prefit[(i, 0)])?;
        }
        // Serialize the postfit
        for i in 0..M::dim() {
            seq.serialize_element(&self.postfit[(i, 0)])?;
        }
        seq.end()
    }
}
