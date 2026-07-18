use clap::Parser;
// includes the REAL parser source (same file main.rs declares as `mod args;`)
#[path = "../src/args.rs"] mod args;
use args::Cli;

#[test]
fn defaults_map_to_default_config() {
    let c = Cli::parse_from(["o4fix", "a.MP4"]).to_config();
    let d = o4core::config::Config::default();
    assert_eq!(c.severe, d.severe);
    assert_eq!(c.noise_band, d.noise_band);
    assert_eq!(c.fast_wide_ramp, d.fast_wide_ramp);
    assert_eq!(c.fast_wide_accel, d.fast_wide_accel);
    assert!(c.handback_cutoff.is_none() && c.optical_noise.is_none());
    assert!(!c.anchor_mode);
}

#[test]
fn m4_profile_via_flags() {
    let c = Cli::parse_from(["o4fix", "a.MP4", "--fast-wide-cutoff", "16"]).to_config();
    assert_eq!(c.fast_wide_cutoff, 16.0);
    assert_eq!(c.fast_wide_accel, 1500.0); // accel gate defaults ON
}

#[test]
fn output_with_multiple_videos_rejected() {
    assert!(Cli::try_parse_from(["o4fix", "a.MP4", "b.MP4", "-o", "x.MP4"])
        .and_then(|c| c.validate()).is_err());
}
