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
                div id="BODY" style=(format!("position: absolute; left: 0px; top: 0px; width: {}px; height: {}px;", self.size_x, self.size_y)) {
                    @for item in &self.elements {
                        (item)
                    }
                }
                div id="DISCO" style="position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; display: none; background-color: #F2F2F288;" {
                    div style="text-align: center; margin-top: 48vh; font-size: 2rem;" {
                        "Disconnected"
                    } 
                }
            }

            script src="/lib/socket.io.js" {}

            script {
                "const DISCO = document.getElementById('DISCO');"
                "const BODY = document.getElementById('BODY');"
                @for n in names {
                    (format!("const {0} = document.getElementById('{0}');", n))
                }

                "let DATA = new Map();"
                "let SCALE = 0;"
                "console.log('Hello Everynya!');"

                "var socket = io();"
                "socket.on('test', function(msg) {"
                    "console.log(msg);"
                "});"

                "socket.on('require-auth', function() {"
                    "console.log('Server requested auth');"
                    (format!("socket.emit('auth-dashboard', '{}');", &self.name))
                    "DISCO.style.display = 'none';"
                "});"


                "function resize_event() {"        
                    "{"
                        // We indent it to prevent name collisions
                        "console.log('Resize Event: ' + window.innerWidth + '/' + window.innerHeight);"
                        (format!("let scale_to_w = window.innerWidth / {};", self.size_x))
                        (format!("let scale_to_h = window.innerHeight / {};", self.size_y))

                        (PreEscaped("if (scale_to_h < scale_to_w) {"))
                            // Window is wider then tall, so we are pillarboxing by offsetting the sides
                            "console.log('Scaling Dashboard to Pillar Boxing (' + scale_to_h + 'x)');"
                            "SCALE = scale_to_h;"
                            (format!("let width = {} * SCALE;", self.size_x))
                            "let gap = (window.innerWidth - width)/2;"
                            "BODY.style.left = gap + 'px';"
                            "BODY.style.top = '0px';"
                            "BODY.style.width = width + 'px';"
                            "BODY.style.height = window.innerHeight + 'px';"
                        "} else {"
                            // Letterboxing instead
                            "console.log('Scaling Dashboard to Letter Boxing (' + scale_to_w + 'x)');"
                            "SCALE = scale_to_w;"
                            (format!("let height = {} * SCALE;", self.size_y))
                            "let gap = (window.innerHeight - height)/2;"
                            "BODY.style.left = '0px';"
                            "BODY.style.top = gap + 'px';"
                            "BODY.style.width = window.innerWidth + 'px';"
                            "BODY.style.height = height + 'px';"
                        "}"
                    "}"

                    @for item in &self.elements {
                        (item.generate_resize_js())
                    }
                "}"
                
                "window.onresize = resize_event;"
                "resize_event();"

                "socket.on('update', function(UP_ARR) {"
                    "const UPDATE = new Map(UP_ARR);"
                    "console.log(UPDATE);"

                    (PreEscaped("UPDATE.forEach((value, key) => DATA.set(key, value));"))
                    
                    @for item in &self.elements {
                        (item.generate_update_js())
                    }
                "});"

                // Disconnect handler
                "socket.on('disconnect', function() {"
                    "console.log('Lost connection');"
                    "DISCO.style.display = 'block';"
                "});"
            }
        }
        
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DashElement {
    pub(crate) name: String,
    pub(crate) x: Property<i64>,
    pub(crate) y: Property<i64>,
    pub(crate) size_x: Property<i64>,
    pub(crate) size_y: Property<i64>,
    pub(crate) visible: Property<bool>,
    pub(crate) element: DashElementType,
}

impl Render for DashElement {
    fn render(&self) -> Markup {
        let name = if let Some(n) = self.normalize_name() {
            n
        } else {
            return html!();
        };

        html! {
            div id=(name) style=(format!("position: absolute; left:{}px; top:{}px; width:{}px; height:{}px;",
                self.x.get_static_value(), self.y.get_static_value(), self.size_x.get_static_value(), self.size_y.get_static_value())) {
                @match &self.element {
                    DashElementType::Square(color) => {
                        div style=(format!("width:100%;height:100%;background:{}", color)) {}
                    },
                    DashElementType::Folder(elements) => {
                        @for item in elements {
                            (item)
                        }
                    },
                    DashElementType::Text(text) => {
                        div { (text.get_static_value()) }
                    }
                }
            }
        }
    }
}

impl DashElement {
    /// Names are reformated to lower case, but are also checked to insure requirements:
    /// ascii alphanumeric with additionally _
    fn normalize_name(&self) -> Option<String> {
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
        let name = if let Some(n) = self.normalize_name() {
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

        match &self.element {
            DashElementType::Folder(elements) => {
                for e in elements {
                    res.extend(e.list_properties());
                }
            },
            DashElementType::Square(_) => {

            },
            DashElementType::Text(text) => {
                text.add_property_handle_to_collection(&mut res);
            }
        }

        self.x.add_property_handle_to_collection(&mut res);
        self.y.add_property_handle_to_collection(&mut res);
        self.size_x.add_property_handle_to_collection(&mut res);
        self.size_y.add_property_handle_to_collection(&mut res);
        self.visible.add_property_handle_to_collection(&mut res);


        res
    }

    fn generate_update_js(&self) -> Markup {
        let name = if let Some(n) = self.normalize_name() {
            n
        } else {
            return html!();
        };

        html!{
            "{"
                // Handling visibility
                @if let Property::Computed(_) = self.visible {

                } @else {
                    (format!("{}.style.display = '{}';", name.as_str(),
                        match self.visible.get_static_value() {
                            true => "block",
                            false => "none"
                        } ))
                }

                
                // Updating internal value
                @match &self.element {
                    DashElementType::Square(color) => (format!("{}.firstElementChild.style.background = '{}';", name.as_str(), color)),
                    DashElementType::Folder(elements) => {
                        @for e in elements {
                            (e.generate_update_js())
                        }
                    },
                    DashElementType::Text(text) => {
                        @if text.is_computed() {
                            (PreEscaped(format!("{}.firstElementChild.textContent = {};", name.as_str(), text.generate_read_js())))
                        }
                    }
                } 
            "}"
        }
    }

    fn generate_resize_js(&self) -> Markup {
        let name = if let Some(n) = self.normalize_name() {
            n
        } else {
            return html!();
        };

        html!{
            // We are in the resize function already,
            // we have access to the update scale value to apply to all dimensions
            "{"
                (PreEscaped(format!("let offset_x = {} * SCALE;", self.x.generate_read_js())))
                (PreEscaped(format!("let offset_y = {} * SCALE;", self.y.generate_read_js())))
                (PreEscaped(format!("let scale_x = {} * SCALE;", self.size_x.generate_read_js())))
                (PreEscaped(format!("let scale_y = {} * SCALE;", self.size_y.generate_read_js())))

                (format!("{}.style.left = offset_x + 'px';", name.as_str()))
                (format!("{}.style.top = offset_y + 'px';", name.as_str()))
                (format!("{}.style.width = scale_x + 'px';", name.as_str()))
                (format!("{}.style.height = scale_y + 'px';", name.as_str()))

            "}"

            // Size in Folders does not constrain the content (except if I at some point implement % scaling)
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

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum DashElementType {
    Square(String),
    Text(Property<String>),
    Folder(Vec<DashElement>)
}


#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum Property<T> {
    Fixed(T),
    Computed(String)
}

impl Property<bool> {
    fn generate_read_js(&self) -> String {
        match self {
            Property::Fixed(val) => {
                val.to_string()
            },
            Property::Computed(_) => {
                if let Some(res) = self.gen_handle_js() {
                    format!("{}.Bool", res)
                } else {
                    self.get_static_value().to_string()
                }
            }
        }
    }
}

impl Property<i64> {
    fn generate_read_js(&self) -> String {
        match self {
            Property::Fixed(val) => {
                val.to_string()
            },
            Property::Computed(_) => {
                if let Some(res) = self.gen_handle_js() {
                    format!("{}", res)
                } else {
                    self.get_static_value().to_string()
                }
            }
        }
    }
}

impl Property<String> {
    fn generate_read_js(&self) -> String {
        match self {
            Property::Fixed(val) => {
                format!("'{}'", val)
            },
            Property::Computed(_) => {
                if let Some(res) = self.gen_handle_js() {
                    format!("{}.Str", res)
                } else {
                    self.get_static_value().to_string()
                }
            }
        }
    }
}

impl<T> Property<T> {
    fn gen_handle_js(&self) -> Option<String> {
        if let Some(handle) = self.get_property_handle() {
            let serial = serde_json::to_string(&handle).ok()?;
            Some(format!("DATA.get({})", serial))
            
        } else {
            None
        }
    }

    pub(crate) fn get_property_handle(&self) -> Option<PropertyHandle> {
        match self {
            Property::Computed(handle) => {
                PropertyHandle::new(handle.as_str())
            },
            _ => None
        }

    }

    pub(crate) fn is_computed(&self) -> bool {
        match self {
            Property::Fixed(_) => false,
            Property::Computed(_) => true
        }
    }

    pub(crate) fn add_property_handle_to_collection(&self, set: &mut HashSet<PropertyHandle>) {
        if let Some(res) = self.get_property_handle() {
            set.insert(res);
        }
    }
}

impl<T> Property<T> where T: Default + Clone {
    pub(crate) fn get_static_value(&self) -> T {
        match self {
            Property::Fixed(res) => res.clone(),
            Property::Computed(_) => {
                T::default()
            }
        }
    }
}
