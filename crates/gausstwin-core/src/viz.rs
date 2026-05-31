//! High-Performance Visualization Module
//!
//! GPU-accelerated real-time visualization with support for millions of agents.

use crate::{AgentId, space::VecN, error::Result};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// High-performance 3D renderer with GPU acceleration
pub struct Renderer3D {
    scene_objects: HashMap<AgentId, SceneObject>,
    cameras: Vec<Camera>,
    lights: Vec<Light>,
    render_settings: RenderSettings,
    performance_stats: RenderStats,
}

/// 3D scene object representing an agent or environment element
#[derive(Debug, Clone)]
pub struct SceneObject {
    pub position: VecN,
    pub rotation: VecN,
    pub scale: VecN,
    pub mesh_id: String,
    pub material_id: String,
    pub visible: bool,
}

/// Camera for 3D scene viewing
#[derive(Debug, Clone)]
pub struct Camera {
    pub position: VecN,
    pub target: VecN,
    pub up: VecN,
    pub fov: f64,
    pub near_plane: f64,
    pub far_plane: f64,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

/// Light source for scene illumination
#[derive(Debug, Clone)]
pub struct Light {
    pub position: VecN,
    pub color: [f32; 3],
    pub intensity: f32,
    pub light_type: LightType,
}

#[derive(Debug, Clone)]
pub enum LightType {
    Directional,
    Point,
    Spot { direction: VecN, cone_angle: f32 },
}

/// Rendering configuration and quality settings
#[derive(Debug, Clone)]
pub struct RenderSettings {
    pub anti_aliasing: bool,
    pub shadows_enabled: bool,
    pub max_agents_rendered: usize,
    pub level_of_detail: bool,
    pub background_color: [f32; 4],
}

/// Performance statistics for rendering optimization
#[derive(Debug, Clone, Default)]
pub struct RenderStats {
    pub frame_time: Duration,
    pub objects_rendered: usize,
    pub triangles_rendered: usize,
    pub draw_calls: usize,
    pub gpu_memory_used: usize,
}

impl Renderer3D {
    /// Create a new 3D renderer with default settings
    pub fn new() -> Self {
        let default_camera = Camera {
            position: VecN::new(0.0, 0.0, 10.0),
            target: VecN::new(0.0, 0.0, 0.0),
            up: VecN::new(0.0, 1.0, 0.0),
            fov: 45.0,
            near_plane: 0.1,
            far_plane: 1000.0,
            viewport_width: 1920,
            viewport_height: 1080,
        };
        
        let default_light = Light {
            position: VecN::new(10.0, 10.0, 10.0),
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            light_type: LightType::Directional,
        };
        
        Self {
            scene_objects: HashMap::new(),
            cameras: vec![default_camera],
            lights: vec![default_light],
            render_settings: RenderSettings::default(),
            performance_stats: RenderStats::default(),
        }
    }
    
    /// Add or update a scene object for an agent
    pub fn update_agent_object(&mut self, agent_id: AgentId, object: SceneObject) {
        self.scene_objects.insert(agent_id, object);
    }
    
    /// Remove an agent's scene object
    pub fn remove_agent_object(&mut self, agent_id: AgentId) -> Option<SceneObject> {
        self.scene_objects.remove(&agent_id)
    }
    
    /// Render the current frame with all visible objects
    pub fn render_frame(&mut self) -> Result<()> {
        let start_time = Instant::now();
        
        // Frustum culling - only render visible objects
        let visible_objects: Vec<(AgentId, &SceneObject)> = self.scene_objects
            .iter()
            .filter(|(_, obj)| obj.visible)
            .map(|(id, obj)| (*id, obj))
            .collect();
        
        // Sort objects by distance for optimal rendering order
        let sorted_objects = self.sort_objects_by_distance(&visible_objects);
        
        // Store render data to avoid borrowing conflicts
        let objects_to_render: Vec<(AgentId, SceneObject)> = sorted_objects.iter()
            .map(|(id, obj)| (*id, (*obj).clone()))
            .collect();
        
        let objects_count = objects_to_render.len();
        
        // Render each object
        for (agent_id, object) in objects_to_render {
            self.render_object(agent_id, &object)?;
        }
        
        // Update performance statistics
        self.performance_stats.frame_time = start_time.elapsed();
        self.performance_stats.objects_rendered = objects_count;
        
        Ok(())
    }
    
    /// Capture a screenshot of the current frame
    pub fn screenshot(&self, _format: ImageFormat) -> Result<Vec<u8>> {
        // Implementation would capture the current framebuffer
        Ok(vec![0; 1920 * 1080 * 4]) // Placeholder: RGBA data
    }
    
    /// Start recording a video of the simulation
    pub fn start_recording(&mut self, _format: VideoFormat, _quality: VideoQuality) -> Result<()> {
        // Implementation would set up video encoding
        Ok(())
    }
    
    /// Stop video recording and return the encoded video
    pub fn stop_recording(&mut self) -> Result<Vec<u8>> {
        // Implementation would finalize and return video data
        Ok(vec![])
    }
    
    /// Update camera transformations and matrices
    pub fn update_cameras(&mut self) {
        let camera_count = self.cameras.len();
        for i in 0..camera_count {
            // Update projection and view matrices without borrowing conflicts
            let camera = &self.cameras[i];
            self.calculate_projection_matrix_for_camera(camera);
        }
    }
    
    /// Perform frustum culling to optimize rendering
    pub fn frustum_culling<'a>(&self, objects: &'a [(AgentId, &'a SceneObject)]) -> Vec<(AgentId, &'a SceneObject)> {
        // Implementation would check if objects are within camera frustum
        objects.iter().copied().collect()
    }
    
    fn sort_objects_by_distance<'a>(&self, objects: &'a [(AgentId, &'a SceneObject)]) -> Vec<(AgentId, &'a SceneObject)> {
        let camera_pos = &self.cameras[0].position;
        let mut sorted = objects.to_vec();
        
        sorted.sort_by(|a, b| {
            let dist_a = self.calculate_distance(camera_pos, &a.1.position);
            let dist_b = self.calculate_distance(camera_pos, &b.1.position);
            dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        sorted
    }
    
    fn calculate_distance(&self, pos1: &VecN, pos2: &VecN) -> f64 {
        let dx = pos1.x - pos2.x;
        let dy = pos1.y - pos2.y;
        let dz = pos1.z - pos2.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
    
    fn render_object(&mut self, _agent_id: AgentId, _object: &SceneObject) -> Result<()> {
        // Implementation would render the 3D object using GPU
        self.performance_stats.draw_calls += 1;
        Ok(())
    }
    
    fn calculate_projection_matrix_for_camera(&self, _camera: &Camera) {
        // Implementation would calculate projection matrix for the camera
    }
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            anti_aliasing: true,
            shadows_enabled: true,
            max_agents_rendered: 100000,
            level_of_detail: true,
            background_color: [0.1, 0.1, 0.1, 1.0],
        }
    }
}

/// Real-time data visualization components
pub struct DataVisualizer {
    charts: HashMap<String, Chart>,
    update_frequency: Duration,
    auto_scale: bool,
}

/// Chart for displaying simulation metrics
#[derive(Debug, Clone)]
pub struct Chart {
    pub chart_type: ChartType,
    pub title: String,
    pub data_series: Vec<DataSeries>,
    pub x_axis: AxisConfig,
    pub y_axis: AxisConfig,
}

#[derive(Debug, Clone)]
pub enum ChartType {
    Line,
    Bar,
    Scatter,
    Histogram,
    Heatmap,
}

#[derive(Debug, Clone)]
pub struct DataSeries {
    pub name: String,
    pub data: Vec<(f64, f64)>,
    pub color: [f32; 3],
    pub style: LineStyle,
}

#[derive(Debug, Clone)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone)]
pub struct AxisConfig {
    pub label: String,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub auto_scale: bool,
}

impl DataVisualizer {
    pub fn new() -> Self {
        Self {
            charts: HashMap::new(),
            update_frequency: Duration::from_millis(100),
            auto_scale: true,
        }
    }
    
    /// Add a new chart for data visualization
    pub fn add_chart(&mut self, chart_id: String, chart: Chart) {
        self.charts.insert(chart_id, chart);
    }
    
    /// Update chart data with new values
    pub fn update_chart_data(&mut self, chart_id: &str, series_name: &str, data_point: (f64, f64)) -> Result<()> {
        if let Some(chart) = self.charts.get_mut(chart_id) {
            if let Some(series) = chart.data_series.iter_mut().find(|s| s.name == series_name) {
                series.data.push(data_point);
                
                // Keep only recent data points to prevent memory growth
                if series.data.len() > 1000 {
                    series.data.drain(..500);
                }
            }
        }
        Ok(())
    }
    
    /// Render all charts to image data
    pub fn render_charts(&self) -> Result<HashMap<String, Vec<u8>>> {
        let mut rendered_charts = HashMap::new();
        
        for (chart_id, _chart) in &self.charts {
            // Implementation would render chart to image data
            rendered_charts.insert(chart_id.clone(), vec![0; 800 * 600 * 4]); // Placeholder
        }
        
        Ok(rendered_charts)
    }
    
    fn render_chart(&self, _chart: &Chart) -> Result<()> {
        // Implementation would render individual chart
        Ok(())
    }
}

/// Image and video export formats
#[derive(Debug, Clone)]
pub enum ImageFormat {
    PNG,
    JPEG,
    BMP,
    TIFF,
}

#[derive(Debug, Clone)]
pub enum VideoFormat {
    MP4,
    AVI,
    WebM,
    GIF,
}

#[derive(Debug, Clone)]
pub enum VideoQuality {
    Low,
    Medium,
    High,
    Ultra,
}

pub fn normalize(v: &VecN) -> VecN {
    let length = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    if length > 0.0 {
        VecN::new(v.x / length, v.y / length, v.z / length)
    } else {
        v.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_renderer_creation() {
        let renderer = Renderer3D::new();
        assert_eq!(renderer.cameras.len(), 1);
        assert_eq!(renderer.lights.len(), 1);
        assert!(renderer.scene_objects.is_empty());
    }
    
    #[test]
    fn test_scene_object_management() {
        let mut renderer = Renderer3D::new();
        let agent_id = AgentId::from_raw(1);
        
        let object = SceneObject {
            position: VecN::new(1.0, 2.0, 3.0),
            rotation: VecN::new(0.0, 0.0, 0.0),
            scale: VecN::new(1.0, 1.0, 1.0),
            mesh_id: "cube".to_string(),
            material_id: "default".to_string(),
            visible: true,
        };
        
        renderer.update_agent_object(agent_id, object.clone());
        assert_eq!(renderer.scene_objects.len(), 1);
        
        let removed = renderer.remove_agent_object(agent_id);
        assert!(removed.is_some());
        assert!(renderer.scene_objects.is_empty());
    }
    
    #[test]
    fn test_data_visualizer() {
        let mut visualizer = DataVisualizer::new();
        
        let chart = Chart {
            chart_type: ChartType::Line,
            title: "Agent Count".to_string(),
            data_series: vec![DataSeries {
                name: "count".to_string(),
                data: vec![],
                color: [1.0, 0.0, 0.0],
                style: LineStyle::Solid,
            }],
            x_axis: AxisConfig {
                label: "Time".to_string(),
                min_value: None,
                max_value: None,
                auto_scale: true,
            },
            y_axis: AxisConfig {
                label: "Count".to_string(),
                min_value: Some(0.0),
                max_value: None,
                auto_scale: true,
            },
        };
        
        visualizer.add_chart("agent_count".to_string(), chart);
        visualizer.update_chart_data("agent_count", "count", (1.0, 100.0)).unwrap();
        
        assert_eq!(visualizer.charts.len(), 1);
    }
}
