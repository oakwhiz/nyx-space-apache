use crate::celestia::Frame;
use crate::dimensions::allocator::Allocator;
use crate::dimensions::{DefaultAllocator, DimName, MatrixMN, VectorN, U3, U6};
use crate::time::{Duration, Epoch};

use std::fmt;

pub type SNC3 = SNC<U3>;
pub type SNC6 = SNC<U6>;

#[derive(Clone)]
pub struct SNC<A: DimName>
where
    DefaultAllocator: Allocator<f64, A> + Allocator<f64, A, A>,
{
    /// Time at which this SNC starts to become applicable
    pub start_time: Option<Epoch>,
    /// Specify the frame of this SNC -- CURRENTLY UNIMPLEMENTED
    pub frame: Option<Frame>,
    /// Enables state noise compensation (process noise) only be applied if the time between measurements is less than the disable_time amount in seconds
    pub disable_time: Duration,
    // Stores the initial epoch when the SNC is requested, needed for decay. Kalman filter will edit this automatically.
    pub init_epoch: Option<Epoch>,
    diag: VectorN<f64, A>,
    decay_diag: Option<Vec<f64>>,
    // Stores the previous epoch of the SNC request, needed for disable time
    pub prev_epoch: Option<Epoch>,
}

impl<A> fmt::Debug for SNC<A>
where
    A: DimName,
    DefaultAllocator: Allocator<f64, A> + Allocator<f64, A, A>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(decay) = &self.decay_diag {
            let mut fmt_cov = Vec::with_capacity(A::dim());
            for (i, dv) in decay.iter().enumerate() {
                fmt_cov.push(format!("{:.1e} × exp(- {:.1e} × t)", self.diag[i], dv));
            }
            write!(
                f,
                "SNC: diag({}) {}",
                fmt_cov.join(", "),
                if let Some(start) = self.start_time {
                    format!("starting at {}", start.as_gregorian_utc_str())
                } else {
                    "".to_string()
                }
            )
        } else {
            let mut fmt_cov = Vec::with_capacity(A::dim());
            for i in 0..A::dim() {
                fmt_cov.push(format!("{:.1e}", self.diag[i]));
            }
            write!(
                f,
                "SNC: diag({}) {}",
                fmt_cov.join(", "),
                if let Some(start) = self.start_time {
                    format!("starting at {}", start.as_gregorian_utc_str())
                } else {
                    "".to_string()
                }
            )
        }
    }
}

impl<A> fmt::Display for SNC<A>
where
    A: DimName,
    DefaultAllocator: Allocator<f64, A> + Allocator<f64, A, A>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl<A: DimName> SNC<A>
where
    DefaultAllocator: Allocator<f64, A> + Allocator<f64, A, A>,
{
    /// Initialize a state noise compensation structure from the diagonal values
    pub fn from_diagonal(disable_time: Duration, values: &[f64]) -> Self {
        assert_eq!(
            values.len(),
            A::dim(),
            "Not enough values for the size of the SNC matrix"
        );

        let mut diag = VectorN::zeros();
        for (i, v) in values.iter().enumerate() {
            diag[i] = *v;
        }

        Self {
            diag,
            disable_time,
            start_time: None,
            frame: None,
            decay_diag: None,
            init_epoch: None,
            prev_epoch: None,
        }
    }

    /// Initialize an SNC with a time at which it should start
    pub fn with_start_time(disable_time: Duration, values: &[f64], start_time: Epoch) -> Self {
        let mut me = Self::from_diagonal(disable_time, values);
        me.start_time = Some(start_time);
        me
    }

    /// Initialize an exponentially decaying SNC with initial SNC and decay constants.
    /// Decay constants in seconds since start of the tracking pass.
    pub fn with_decay(
        disable_time: Duration,
        initial_snc: &[f64],
        decay_constants_s: &[f64],
    ) -> Self {
        assert_eq!(
            decay_constants_s.len(),
            A::dim(),
            "Not enough decay constants for the size of the SNC matrix"
        );

        let mut me = Self::from_diagonal(disable_time, initial_snc);
        me.decay_diag = Some(decay_constants_s.to_vec());
        me
    }

    /// Returns the SNC matrix (_not_ incl. Gamma matrix approximation) at the provided Epoch.
    /// May be None if:
    ///  1. Start time of this matrix is _after_ epoch
    ///  2. Time between epoch and previous epoch (set in the Kalman filter!) is longer than disabling time
    pub fn to_matrix(&self, epoch: Epoch) -> Option<MatrixMN<f64, A, A>> {
        if let Some(start_time) = self.start_time {
            if start_time > epoch {
                // This SNC applies only later
                return None;
            }
        }

        // Check the disable time, and return no SNC if the previous SNC was computed too long ago
        if let Some(prev_epoch) = self.prev_epoch {
            if epoch - prev_epoch > self.disable_time {
                return None;
            }
        }
        // Build a static matrix
        let mut snc = MatrixMN::<f64, A, A>::zeros();
        for i in 0..self.diag.nrows() {
            snc[(i, i)] = self.diag[i];
        }

        if let Some(decay) = &self.decay_diag {
            // Let's apply the decay to the diagonals
            let total_delta_t = (epoch - self.init_epoch.unwrap()).in_seconds();
            for i in 0..self.diag.nrows() {
                snc[(i, i)] *= (-decay[i] * total_delta_t).exp();
            }
        }

        Some(snc)
    }
}

#[test]
fn test_snc_init() {
    use crate::time::TimeUnit;
    let snc_expo = SNC3::with_decay(
        2 * TimeUnit::Minute,
        &[1e-6, 1e-6, 1e-6],
        &[3600.0, 3600.0, 3600.0],
    );
    println!("{}", snc_expo);

    let snc_std = SNC3::with_start_time(
        2 * TimeUnit::Minute,
        &[1e-6, 1e-6, 1e-6],
        Epoch::from_et_seconds(3600.0),
    );
    println!("{}", snc_std);
}
