use crate::mihomo;
use crate::mihomo::{LocalClashProbe, MihomoControllerInput, MihomoSnapshot};

pub(crate) fn inspect_mihomo_controller(
    input: MihomoControllerInput,
) -> Result<MihomoSnapshot, String> {
    mihomo::inspect_controller(&input).map_err(|error| error.to_string())
}

pub(crate) fn probe_local_clash(secret: Option<String>) -> LocalClashProbe {
    mihomo::probe_local_clash(secret.as_deref().unwrap_or(""))
}
