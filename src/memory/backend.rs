/// Memory backend kind -- Oracle is the only option.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MemoryBackendKind {
    Oracle,
}

/// Memory backend profile for the onboarding wizard.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MemoryBackendProfile {
    pub key: &'static str,
    pub label: &'static str,
    pub auto_save_default: bool,
}

const ORACLE_PROFILE: MemoryBackendProfile = MemoryBackendProfile {
    key: "oracle",
    label: "Oracle AI Database -- in-database ONNX vector search, persistent memory",
    auto_save_default: true,
};

pub fn selectable_memory_backends() -> &'static [MemoryBackendProfile] {
    &[ORACLE_PROFILE]
}

pub fn default_memory_backend_key() -> &'static str {
    ORACLE_PROFILE.key
}

pub fn classify_memory_backend(_backend: &str) -> MemoryBackendKind {
    MemoryBackendKind::Oracle
}

pub fn memory_backend_profile(_backend: &str) -> MemoryBackendProfile {
    ORACLE_PROFILE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_always_returns_oracle() {
        assert_eq!(classify_memory_backend("sqlite"), MemoryBackendKind::Oracle);
        assert_eq!(classify_memory_backend("oracle"), MemoryBackendKind::Oracle);
        assert_eq!(
            classify_memory_backend("anything"),
            MemoryBackendKind::Oracle
        );
    }

    #[test]
    fn default_backend_is_oracle() {
        assert_eq!(default_memory_backend_key(), "oracle");
    }

    #[test]
    fn selectable_has_only_oracle() {
        let backends = selectable_memory_backends();
        assert_eq!(backends.len(), 1);
        assert_eq!(backends[0].key, "oracle");
    }
}
