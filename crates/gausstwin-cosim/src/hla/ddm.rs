use super::{RegionConfig, RegionHandle};

#[derive(Debug)]
pub struct DdmManager;

impl DdmManager {
    pub fn new(_config: &super::HlaDataDistribution) -> Self {
        DdmManager
    }
    pub fn create_region(
        &mut self,
        _handle: RegionHandle,
        _config: &RegionConfig,
    ) -> Result<(), String> {
        Ok(())
    }
}
