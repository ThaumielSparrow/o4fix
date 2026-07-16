use thiserror::Error;

#[derive(Error, Debug)]
pub enum O4Error {
    #[error("No DJI O4 telemetry found ({0}) — is this an O4 Pro recording?")]
    NoTelemetry(String),
    #[error("Couldn't calibrate motion from this clip (needs some clean flight sections){}",
            .r2.map(|r| format!(" — alignment R2={r:.3} < 0.8")).unwrap_or_default())]
    CalibrationFailed { r2: Option<f64> },
    #[error("round-trip verification failed; output deleted")]
    VerifyFailed,
    #[error("cancelled")]
    Cancelled,
    #[error("MP4 structure error: {0}")]
    Mp4(String),
    #[error("telemetry parse error: {0}")]
    Telemetry(String),
    #[error("OpenCV error: {0}")]
    Cv(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
