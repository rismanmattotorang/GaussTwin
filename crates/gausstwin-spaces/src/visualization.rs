use crate::{Point, SpatialError, SpatialResult};
use kiss3d::{
    camera::{ArcBall, Camera},
    light::Light,
    nalgebra::{Point3, Translation3, UnitQuaternion, Vector3},
    planar_camera::PlanarCamera,
    post_processing::PostProcessingEffect,
    scene::SceneNode,
    window::Window,
};
use plotters::{
    prelude::*,
    style::{Color, RGBColor},
};
use std::{
    collections::HashMap,
    f64::consts::PI,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// Configuration for visualization
#[derive(Debug, Clone)]
pub struct VisualizationConfig {
    /// Window title
    pub window_title: String,
    /// Window size
    pub window_size: (u32, u32),
    /// Background color (R, G, B)
    pub background_color: (f32, f32, f32),
    /// Point size for rendering
    pub point_size: f32,
    /// Camera settings
    pub camera_config: CameraConfig,
    /// Rendering quality
    pub rendering_quality: RenderingQuality,
    /// Animation settings
    pub animation_config: AnimationConfig,
}

impl Default for VisualizationConfig {
    fn default() -> Self {
        Self {
            window_title: "GaussTwin Visualization".to_string(),
            window_size: (1024, 768),
            background_color: (0.1, 0.1, 0.1),
            point_size: 0.1,
            camera_config: CameraConfig::default(),
            rendering_quality: RenderingQuality::High,
            animation_config: AnimationConfig::default(),
        }
    }
}

/// Camera configuration
#[derive(Debug, Clone)]
pub struct CameraConfig {
    /// Initial eye position
    pub eye: Point3<f32>,
    /// Initial target position
    pub target: Point3<f32>,
    /// Up vector
    pub up: Vector3<f32>,
    /// Field of view (in radians)
    pub fov: f32,
    /// Near plane distance
    pub znear: f32,
    /// Far plane distance
    pub zfar: f32,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            eye: Point3::new(10.0, 10.0, 10.0),
            target: Point3::origin(),
            up: Vector3::y(),
            fov: PI / 4.0,
            znear: 0.1,
            zfar: 1000.0,
        }
    }
}

/// Rendering quality settings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderingQuality {
    Low,
    Medium,
    High,
    Ultra,
}

impl RenderingQuality {
    fn get_settings(&self) -> RenderingSettings {
        match self {
            Self::Low => RenderingSettings {
                msaa_samples: 1,
                shadow_mapping: false,
                ambient_occlusion: false,
                bloom: false,
            },
            Self::Medium => RenderingSettings {
                msaa_samples: 2,
                shadow_mapping: true,
                ambient_occlusion: false,
                bloom: false,
            },
            Self::High => RenderingSettings {
                msaa_samples: 4,
                shadow_mapping: true,
                ambient_occlusion: true,
                bloom: true,
            },
            Self::Ultra => RenderingSettings {
                msaa_samples: 8,
                shadow_mapping: true,
                ambient_occlusion: true,
                bloom: true,
            },
        }
    }
}

/// Rendering settings
#[derive(Debug, Clone, Copy)]
struct RenderingSettings {
    msaa_samples: u8,
    shadow_mapping: bool,
    ambient_occlusion: bool,
    bloom: bool,
}

/// Animation configuration
#[derive(Debug, Clone)]
pub struct AnimationConfig {
    /// Animation duration in seconds
    pub duration: f32,
    /// Animation easing function
    pub easing: EasingFunction,
    /// Frame rate
    pub fps: u32,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            duration: 1.0,
            easing: EasingFunction::EaseInOutCubic,
            fps: 60,
        }
    }
}

/// Easing functions for animations
#[derive(Debug, Clone, Copy)]
pub enum EasingFunction {
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
}

impl EasingFunction {
    fn apply(&self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::EaseInQuad => t * t,
            Self::EaseOutQuad => t * (2.0 - t),
            Self::EaseInOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    -1.0 + (4.0 - 2.0 * t) * t
                }
            }
            Self::EaseInCubic => t * t * t,
            Self::EaseOutCubic => {
                let t1 = t - 1.0;
                1.0 + t1 * t1 * t1
            }
            Self::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let t1 = t - 1.0;
                    1.0 + 4.0 * t1 * t1 * t1
                }
            }
        }
    }
}

/// Visualization manager
pub struct Visualizer {
    config: VisualizationConfig,
    window: Window,
    camera: ArcBall,
    objects: HashMap<usize, SceneNode>,
    animations: Vec<Animation>,
    last_update: Instant,
}

impl Visualizer {
    /// Create a new visualizer
    pub fn new(config: VisualizationConfig) -> SpatialResult<Self> {
        let mut window = Window::new(&config.window_title);
        window.set_background_color(
            config.background_color.0,
            config.background_color.1,
            config.background_color.2,
        );

        let camera = ArcBall::new(config.camera_config.eye, config.camera_config.target);

        let mut visualizer = Self {
            config,
            window,
            camera,
            objects: HashMap::new(),
            animations: Vec::new(),
            last_update: Instant::now(),
        };

        visualizer.setup_scene()?;
        Ok(visualizer)
    }

    /// Setup the initial scene
    fn setup_scene(&mut self) -> SpatialResult<()> {
        // Add ambient light
        self.window.set_light(Light::StickToCamera);

        // Setup post-processing effects based on quality settings
        let settings = self.config.rendering_quality.get_settings();
        if settings.bloom {
            self.window
                .set_post_processing_effect(PostProcessingEffect::Bloom(0.5));
        }

        Ok(())
    }

    /// Add a point to the visualization
    pub fn add_point(
        &mut self,
        id: usize,
        point: Point,
        color: (f32, f32, f32),
    ) -> SpatialResult<()> {
        let mut node = self.window.add_sphere(self.config.point_size);
        node.set_color(color.0, color.1, color.2);
        node.set_local_translation(Translation3::new(
            point.x as f32,
            point.y as f32,
            point.z as f32,
        ));
        self.objects.insert(id, node);
        Ok(())
    }

    /// Remove a point from the visualization
    pub fn remove_point(&mut self, id: usize) -> SpatialResult<()> {
        if let Some(node) = self.objects.remove(&id) {
            node.unlink();
        }
        Ok(())
    }

    /// Update point position with animation
    pub fn update_point(
        &mut self,
        id: usize,
        new_position: Point,
        duration: Option<f32>,
    ) -> SpatialResult<()> {
        let node = self.objects.get(&id).ok_or(SpatialError::InvalidOperation(
            "Point not found".to_string(),
        ))?;

        let current_pos = node.local_translation();
        let target_pos = Translation3::new(
            new_position.x as f32,
            new_position.y as f32,
            new_position.z as f32,
        );

        let duration = duration.unwrap_or(self.config.animation_config.duration);
        let animation = Animation {
            node: node.clone(),
            start_pos: current_pos,
            target_pos,
            start_time: Instant::now(),
            duration,
            easing: self.config.animation_config.easing,
        };

        self.animations.push(animation);
        Ok(())
    }

    /// Update the visualization
    pub fn update(&mut self) -> SpatialResult<bool> {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        // Update animations
        self.animations.retain_mut(|animation| {
            let elapsed = animation.start_time.elapsed().as_secs_f32();
            let progress = (elapsed / animation.duration).min(1.0);
            let t = animation.easing.apply(progress);

            let current_pos = animation.start_pos.lerp(&animation.target_pos, t);
            animation.node.set_local_translation(current_pos);

            progress < 1.0
        });

        // Render frame
        self.window.render_with_camera(&mut self.camera);

        Ok(!self.window.should_close())
    }

    /// Save the current view to an image file
    pub fn save_screenshot(&self, path: &str) -> SpatialResult<()> {
        self.window.snap_image(path).map_err(|_| {
            SpatialError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to save screenshot",
            ))
        })
    }

    /// Create a 2D plot of points
    pub fn create_2d_plot(
        &self,
        points: &[Point],
        path: &str,
        width: u32,
        height: u32,
    ) -> SpatialResult<()> {
        let root = BitMapBackend::new(path, (width, height)).into_drawing_area();
        root.fill(&WHITE)?;

        let (min_x, max_x, min_y, max_y) = points.iter().fold(
            (f64::MAX, f64::MIN, f64::MAX, f64::MIN),
            |(min_x, max_x, min_y, max_y), p| {
                (
                    min_x.min(p.x),
                    max_x.max(p.x),
                    min_y.min(p.y),
                    max_y.max(p.y),
                )
            },
        );

        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(30)
            .build_cartesian_2d(min_x..max_x, min_y..max_y)?;

        chart.configure_mesh().draw()?;

        chart.draw_series(
            points
                .iter()
                .map(|p| Circle::new((p.x, p.y), 3, ShapeStyle::from(&BLACK).filled())),
        )?;

        root.present()?;
        Ok(())
    }
}

/// Animation state
struct Animation {
    node: SceneNode,
    start_pos: Translation3<f32>,
    target_pos: Translation3<f32>,
    start_time: Instant,
    duration: f32,
    easing: EasingFunction,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn test_visualization_config() {
        let config = VisualizationConfig::default();
        assert_eq!(config.window_size, (1024, 768));
        assert_eq!(config.rendering_quality, RenderingQuality::High);
    }

    #[test]
    fn test_easing_functions() {
        let t = 0.5;
        assert_eq!(EasingFunction::Linear.apply(t), 0.5);
        assert!((EasingFunction::EaseInQuad.apply(t) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_rendering_settings() {
        let settings = RenderingQuality::Ultra.get_settings();
        assert_eq!(settings.msaa_samples, 8);
        assert!(settings.shadow_mapping);
        assert!(settings.ambient_occlusion);
        assert!(settings.bloom);
    }

    #[test]
    fn test_camera_config() {
        let config = CameraConfig::default();
        assert_eq!(config.fov, PI / 4.0);
        assert_eq!(config.znear, 0.1);
        assert_eq!(config.zfar, 1000.0);
    }
}
