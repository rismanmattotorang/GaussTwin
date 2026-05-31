use super::{AttributeValues, ObjectInstanceHandle};
use crate::common::data::DataValue;
use std::collections::HashMap;

#[derive(Debug)]
pub struct ObjectManager;

impl ObjectManager {
    pub fn new() -> Self {
        ObjectManager
    }
    pub fn register_object(
        &mut self,
        _handle: ObjectInstanceHandle,
        _class_name: &str,
        _attributes: &[&str],
    ) -> Result<(), String> {
        Ok(())
    }
    pub fn prepare_attribute_values(
        &self,
        _handle: ObjectInstanceHandle,
        _attributes: HashMap<String, DataValue>,
    ) -> Result<AttributeValues, String> {
        Ok(AttributeValues(HashMap::new()))
    }
    pub fn discover_object(
        &mut self,
        _handle: ObjectInstanceHandle,
        _class_name: &str,
    ) -> Result<(), String> {
        Ok(())
    }
    pub fn reflect_attributes(
        &mut self,
        _handle: ObjectInstanceHandle,
        _attributes: AttributeValues,
    ) -> Result<(), String> {
        Ok(())
    }
}
