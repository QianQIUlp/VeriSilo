use crate::mihomo;
use crate::mihomo::{LocalClashProbe, MihomoControllerInput, MihomoError, MihomoSnapshot};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum MihomoControllerErrorCode {
    ControllerUnreachable,
    ControllerTransport,
    MixedPort,
    ControllerRejected,
    ControllerInternal,
}

/// Error contract for controller inspection only; display text never decides rediscovery.
#[derive(Debug, serde::Serialize)]
pub(crate) struct MihomoControllerError {
    code: MihomoControllerErrorCode,
    message: String,
}

impl From<MihomoError> for MihomoControllerError {
    fn from(error: MihomoError) -> Self {
        let code = match &error {
            MihomoError::ControllerUnreachable => MihomoControllerErrorCode::ControllerUnreachable,
            MihomoError::Io(_) => MihomoControllerErrorCode::ControllerTransport,
            MihomoError::MixedPortUsedAsController(_) => MihomoControllerErrorCode::MixedPort,
            // Authentication, invalid input and protocol failures must keep the selected target.
            _ => MihomoControllerErrorCode::ControllerRejected,
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

impl MihomoControllerError {
    pub(crate) fn worker_failed(message: String) -> Self {
        Self {
            code: MihomoControllerErrorCode::ControllerInternal,
            message,
        }
    }
}

pub(crate) fn inspect_mihomo_controller(
    input: MihomoControllerInput,
) -> Result<MihomoSnapshot, MihomoControllerError> {
    mihomo::inspect_controller(&input).map_err(Into::into)
}

pub(crate) fn probe_local_clash(secret: Option<String>) -> LocalClashProbe {
    mihomo::probe_local_clash(secret.as_deref().unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_error_contract_preserves_display_and_classifies_rediscovery() {
        for (error, code) in [
            (MihomoError::ControllerUnreachable, "controller_unreachable"),
            (
                MihomoError::Io(std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    "refused",
                )),
                "controller_transport",
            ),
            (MihomoError::MixedPortUsedAsController(7890), "mixed_port"),
            (MihomoError::HttpStatus(401), "controller_rejected"),
            (MihomoError::HttpStatus(403), "controller_rejected"),
            (MihomoError::InvalidSecret, "controller_rejected"),
            (MihomoError::UnsafeController, "controller_rejected"),
            (MihomoError::InvalidResponse, "controller_rejected"),
        ] {
            let message = error.to_string();
            let serialized = serde_json::to_value(MihomoControllerError::from(error)).unwrap();
            assert_eq!(
                serialized,
                serde_json::json!({ "code": code, "message": message })
            );
        }
        let serialized = serde_json::to_value(MihomoControllerError::worker_failed(
            "worker stopped".to_owned(),
        ))
        .unwrap();
        assert_eq!(serialized["code"], "controller_internal");
    }
}
