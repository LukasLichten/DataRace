use hashbrown::HashSet;
use log::error;
use maud::{html, Markup, PreEscaped, Render, DOCTYPE};
use serde::{Deserialize, Serialize};

use crate::PropertyHandle;

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
    pub(crate) elements: Vec<DashElement>,
    pub(crate) size_x: i32,
    pub(crate) size_y: i32
}

impl Dashboard {
    pub(crate) fn list_properties(&self) -> HashSet<PropertyHandle> {
        let mut res = HashSet::<PropertyHandle>::new();

        for e in &self.elements {
            res.extend(e.list_properties());
        }

        res
    }
}

impl Render for Dashboard {
    fn render(&self) -> Markup {
    
        let mut names = vec![];
        for e in &self.elements {
            if !e.gather_names(&mut names) {
                error!("Failed to render Dashboard {} due to element name issues!", self.name);
                return html!{
                    (header(&"Error!".to_string()))
                };
            }
        }

        html! {
            (header(&self.name))
            body {
                div id="DISCO" sytle="position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; display: none; background-color:grey;" {
                    div style="position: absolute; left: 40%; top: 50%" {
                        "Disconnected"
                    } 
                }
                div id="BODY" style=(format!("position: absolute; left: 0px; top: 0px; width: {}px; height: {}px;", self.size_x, self.size_y)) {
                    @for item in &self.elements {
                        (item)
                    }
                }
            }

            script src="/lib/socket.io.js" {}

            script {
                "const BODY = document.getElementById('BODY');"
                @for n in names {
                    (format!("const {0} = document.getElementById('{0}');", n))
                }

                "let scale = 0;"
                "console.log('Hello Everynya!');"

                "var socket = io();"
                "socket.on('test', function(msg) {"
                    "console.log(msg);"
                "});"

                "socket.on('require-auth', function() {"
                    "console.log('Server requested auth');"
                    (format!("socket.emit('auth-dashboard', '{}');", &self.name))
                "});"


                "function resize_event() {"        
                    (format!("let scale_to_w = window.innerWidth / {};", self.size_x))
                    (format!("let scale_to_h = window.innerHeight / {};", self.size_y))

                    (PreEscaped("if (scale_to_h < scale_to_w) {"))
                        // Window is wider then tall, so we are pillarboxing by offsetting the sides
                        "console.log('Scaling Dashboard to Pillar Boxing');"
                        "scale = scale_to_h;"
                        (format!("let width = {} * scale;", self.size_x))
                        "let gap = (window.innerWidth - width)/2;"
                        "BODY.style.left = gap + 'px';"
                        "BODY.style.top = '0px';"
                        "BODY.style.width = width + 'px';"
                        "BODY.style.height = window.innerHeight + 'px';"
                    "} else {"
                        // Letterboxing instead
                        "console.log('Scaling Dashboard to Letter Boxing');"
                        "scale = scale_to_w;"
                        (format!("let height = {} * scale;", self.size_y))
                        "let gap = (window.innerHeight - height)/2;"
                        "BODY.style.left = '0px';"
                        "BODY.style.top = height + 'px';"
                        "BODY.style.width = window.innerWidth + 'px';"
                        "BODY.style.height = height + 'px';"
                    "}"

                    @for item in &self.elements {
                        (item.generate_resize_js())
                    }
                "}"
                
                "window.onresize = resize_event;"
                "resize_event();"

                "function update_cycle() {"
                    "console.log('Update Cycle');"
                    @for item in &self.elements {
                        (item.generate_update_js())
                    }
                "}"
                "update_cycle();"

                "socket.on('update', function(data) {"
                    "console.log(data);"
                "});"


            }
        }
        
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DashElement {
    pub(crate) name: String,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) size_x: i32,
    pub(crate) size_y: i32,
    pub(crate) element: DashElementType,
}

impl Render for DashElement {
    fn render(&self) -> Markup {
        let name = if let Some(n) = self.normilze_name() {
            n
        } else {
            return html!();
        };

        html! {
            div id=(name) style=(format!("position: absolute; left:{}px; top:{}px; width:{}px; height:{}px;", self.x, self.y, self.size_x, self.size_y)) {
                @match &self.element {
                    DashElementType::Square(_color) => {
                        div style=(format!("width:100%;height:100%;")) {}
                    },
                    DashElementType::Folder(elements) => {
                        @for item in elements {
                            (item)
                        }
                    },
                    // _ => {
                    //     
                    // }
                }
            }
        }
    }
}

impl DashElement {
    /// Names are reformated to lower case, but are also checked to insure requirements:
    /// ascii alphanumeric with additionally _
    fn normilze_name(&self) -> Option<String> {
        let name = self.name.to_lowercase();
        
        if !name.chars().all(|x| x.is_ascii_digit() || x.is_ascii_lowercase() || x == '_') {
            error!("Unable to render dashboard: Name '{}' containes illegal characters (only ascii alphabet, numbers and _ permitted)", name);
            return None;
        }

        return Some(name);
    }

    /// Gathers up the name of this element (and any potential sub elements)
    /// and insures there are no name collisions
    fn gather_names(&self, list: &mut Vec<String>) -> bool {
        let name = if let Some(n) = self.normilze_name() {
            n
        } else {
            return false;
        };

        if list.contains(&name) {
            error!("Unable to render dashboard: Unique Name violated with name '{}'", name);
            return false;
        }

        list.push(name);

        if let DashElementType::Folder(elements) = &self.element {
            for e in elements {
                if !e.gather_names(list) {
                    return false;
                }
            }
        }

        true
    }
    
    /// Returns a list of all properties used in scripts for this element
    /// and all elements contained in it
    fn list_properties(&self) -> HashSet<PropertyHandle> {
        let mut res = HashSet::<PropertyHandle>::new();

        if let DashElementType::Folder(elements) = &self.element {
            for e in elements {
                res.extend(e.list_properties());
            }
        }

        // TODO aquire property handle

        res
    }

    fn generate_update_js(&self) -> Markup {
        let name = if let Some(n) = self.normilze_name() {
            n
        } else {
            return html!();
        };

        html!{
            "{"
                (format!("{}.style.display = 'block';", name.as_str()))
                @if let DashElementType::Square(color) = &self.element {
                    (format!("{}.firstElementChild.style.background = '{}';", name.as_str(), color))
                } @else if let DashElementType::Folder(elements) = &self.element {
                    // TODO: Add if clause to not update if this folder is hidden
                    @for e in elements {
                        (e.generate_update_js())
                    }
                }
            "}"
        }
    }

    fn generate_resize_js(&self) -> Markup {
        let name = if let Some(n) = self.normilze_name() {
            n
        } else {
            return html!();
        };

        html!{
            // We are in the resize function already,
            // we have access to the update scale value to apply to all dimensions
            "{"
                (format!("let offset_x = {} * scale;", self.x))
                (format!("let offset_y = {} * scale;", self.y))
                (format!("let scale_x = {} * scale;", self.size_x))
                (format!("let scale_y = {} * scale;", self.size_y))

                (format!("{}.style.left = offset_x + 'px';", name.as_str()))
                (format!("{}.style.top = offset_y + 'px';", name.as_str()))
                (format!("{}.style.width = scale_x + 'px';", name.as_str()))
                (format!("{}.style.height = scale_y + 'px';", name.as_str()))

            "}"

            @if let DashElementType::Folder(elements) = &self.element {
                "{"
                    @for e in elements {
                        (e.generate_resize_js())
                    }
                "}"
            }
        }
    }
}

/// Size in Folders does not constrain the content (except if I at some point implement % scaling)
#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum DashElementType {
    Square(String),
    Folder(Vec<DashElement>)
}


pub(crate) enum Property<T> {
    Fixed(T),
    Computed(String)
}

impl Property<i64> {
    pub fn get_value(&self) -> i64 {
        match self {
            Property::Fixed(res) => res.clone(),
            Property::Computed(prop) => {
                0
            }
        }
    }
}
