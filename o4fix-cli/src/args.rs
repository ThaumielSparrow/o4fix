use clap::Parser;
use o4core::config::Config;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "o4fix",
    version,
    about = "Repair DJI O4 Pro gyro noise: writes VIDEO_fixed.MP4 with \
                   clean embedded telemetry - load it in Gyroflow like a stock recording"
)]
pub struct Cli {
    /// DJI O4 Pro .MP4 file(s)
    #[arg(required = true)]
    pub videos: Vec<PathBuf>,
    /// output path (single video only); default VIDEO_fixed.MP4
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    // MP4 repair tuning
    #[arg(long, default_value_t = 8.0)]
    pub severe: f64,
    #[arg(long, default_value_t = 0.2)]
    pub severe_pad: f64,
    #[arg(long, default_value_t = 0.2)]
    pub severe_merge: f64,
    #[arg(long, default_value_t = 0.3)]
    pub ramp: f64,

    // filter tuning (defaults tuned on O4 Pro test flight)
    #[arg(long, default_value_t = 25.0)]
    pub light_cutoff: f64,
    #[arg(long, default_value_t = 2.5)]
    pub strong_cutoff: f64,
    #[arg(long, default_value_t = 1.5)]
    pub noise_low: f64,
    #[arg(long, default_value_t = 5.0)]
    pub noise_high: f64,
    #[arg(long, num_args = 2, value_names = ["LO", "HI"],
          default_values_t = [30.0, 180.0])]
    pub noise_band: Vec<f64>,
    #[arg(long, default_value_t = 100.0)]
    pub noise_window: f64,
    #[arg(long, default_value_t = 7)]
    pub hampel_window: usize,
    #[arg(long, default_value_t = 6.0)]
    pub hampel_sigma: f64,
    #[arg(long, default_value_t = 8.0)]
    pub optical_cutoff: f64,
    #[arg(long, num_args = 2, value_names = ["LO", "HI"],
          default_values_t = [100.0, 250.0])]
    pub fast_handback: Vec<f64>,
    #[arg(long, default_value_t = 0.5)]
    pub patch_pad: f64,
    #[arg(long, default_value_t = 1.0)]
    pub patch_merge: f64,
    #[arg(long, num_args = 2, value_names = ["LO", "HI"])]
    pub optical_noise: Option<Vec<f64>>,
    #[arg(long)]
    pub handback_cutoff: Option<f64>,
    #[arg(long, default_value_t = 0.0)]
    pub fast_wide_cutoff: f64,
    #[arg(long, num_args = 2, value_names = ["LO", "HI"],
          default_values_t = [150.0, 300.0])]
    pub fast_wide_ramp: Vec<f64>,
    #[arg(long, default_value_t = 1500.0)]
    pub fast_wide_accel: f64,
    #[arg(long)]
    pub anchor_mode: bool,
    #[arg(long, default_value_t = 1.5)]
    pub anchor_cutoff: f64,
}

impl Cli {
    pub fn validate(self) -> Result<Self, clap::Error> {
        if self.output.is_some() && self.videos.len() > 1 {
            // mirrors argparse p.error(...): usage error, exit code 2
            return Err(clap::Error::raw(
                clap::error::ErrorKind::ArgumentConflict,
                "-o/--output only works with a single video\n",
            ));
        }
        Ok(self)
    }
    pub fn to_config(&self) -> Config {
        Config {
            severe: self.severe,
            severe_pad: self.severe_pad,
            severe_merge: self.severe_merge,
            ramp: self.ramp,
            light_cutoff: self.light_cutoff,
            strong_cutoff: self.strong_cutoff,
            noise_low: self.noise_low,
            noise_high: self.noise_high,
            noise_band: (self.noise_band[0], self.noise_band[1]),
            noise_window_ms: self.noise_window,
            hampel_window: self.hampel_window,
            hampel_sigma: self.hampel_sigma,
            optical_cutoff: self.optical_cutoff,
            handback_cutoff: self.handback_cutoff,
            fast_handback: (self.fast_handback[0], self.fast_handback[1]),
            patch_pad: self.patch_pad,
            patch_merge: self.patch_merge,
            optical_noise: self.optical_noise.as_ref().map(|v| (v[0], v[1])),
            fast_wide_cutoff: self.fast_wide_cutoff,
            fast_wide_ramp: (self.fast_wide_ramp[0], self.fast_wide_ramp[1]),
            fast_wide_accel: self.fast_wide_accel,
            anchor_mode: self.anchor_mode,
            anchor_cutoff: self.anchor_cutoff,
        }
    }
}
