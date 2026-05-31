use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type DashboardId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub title: String,
    pub description: String,
    pub refresh_rate: u64,
    pub widgets: Vec<WidgetConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetConfig {
    pub widget_type: WidgetType,
    pub title: String,
    pub data_source: DataSource,
    pub visualization: VisualizationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WidgetType {
    TimeSeries,
    KPI,
    Alert,
    Gauge,
    Table,
    Chart,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSource {
    Metrics(String),
    Query(String),
    Stream(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisualizationType {
    LineChart,
    BarChart,
    PieChart,
    Heatmap,
    ScatterPlot,
    Custom(String),
}

#[derive(Debug)]
pub struct Dashboard {
    id: DashboardId,
    pub config: DashboardConfig,
    widgets: HashMap<String, Widget>,
    last_update: DateTime<Utc>,
}

#[derive(Debug)]
struct Widget {
    config: WidgetConfig,
    data: Vec<DataPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    timestamp: DateTime<Utc>,
    value: f64,
    metadata: HashMap<String, String>,
}

impl Dashboard {
    pub fn new(config: DashboardConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            widgets: config
                .widgets
                .iter()
                .map(|w| {
                    (
                        w.title.clone(),
                        Widget {
                            config: w.clone(),
                            data: Vec::new(),
                        },
                    )
                })
                .collect(),
            config,
            last_update: Utc::now(),
        }
    }

    pub fn id(&self) -> DashboardId {
        self.id
    }

    pub async fn update(&mut self) -> crate::Result<()> {
        // Collect data sources first to avoid borrow checker issues
        let mut widget_updates = Vec::new();

        for (title, widget) in &self.widgets {
            let data = match &widget.config.data_source {
                DataSource::Metrics(path) => self.fetch_metrics(path).await?,
                DataSource::Query(query) => self.execute_query(query).await?,
                DataSource::Stream(stream) => self.process_stream(stream).await?,
            };
            widget_updates.push((title.clone(), data));
        }

        // Now update the widgets with collected data
        for (title, data) in widget_updates {
            if let Some(widget) = self.widgets.get_mut(&title) {
                widget.data.extend(data);
            }
        }

        self.last_update = Utc::now();
        Ok(())
    }

    async fn fetch_metrics(&self, _path: &str) -> crate::Result<Vec<DataPoint>> {
        // TODO: Implement metrics fetching
        Ok(Vec::new())
    }

    async fn execute_query(&self, _query: &str) -> crate::Result<Vec<DataPoint>> {
        // TODO: Implement query execution
        Ok(Vec::new())
    }

    async fn process_stream(&self, _stream: &str) -> crate::Result<Vec<DataPoint>> {
        // TODO: Implement stream processing
        Ok(Vec::new())
    }

    pub fn render(&self) -> crate::Result<String> {
        // TODO: Implement dashboard rendering
        Ok(String::new())
    }
}
