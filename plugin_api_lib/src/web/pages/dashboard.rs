use maud::{html, Render, Markup, DOCTYPE};
use serde::{Deserialize, Serialize};

fn header(name: &String) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        title { "DataRace - " (name) }
    }
} 

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Dashboard {
    pub(crate) name: String,
    pub(crate) elements: Vec<DashElement>
}

impl Render for Dashboard {
    fn render(&self) -> Markup {
        html! {
            (header(&self.name))
            @for item in &self.elements {
                (item)
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DashElement {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) size_x: i32,
    pub(crate) size_y: i32,
    pub(crate) element: DashElementType,
}

impl Render for DashElement {
    fn render(&self) -> Markup {
        html! {
            @match self.element {
                DashElementType::Square => {
                    div style=(format!("position: absolute; left:{}px; top:{}px; width:{}px; height:{}px;background: yellow;", self.x, self.y, self.size_x, self.size_y));
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum DashElementType {
    Square
}


