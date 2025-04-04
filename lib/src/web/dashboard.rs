use log::error;
use maud::{html, Markup, PreEscaped, DOCTYPE};

use datarace_dashboard_spec::{Action, DashElement, DashElementType, Dashboard, Property, Text};

use crate::PropertyHandle;

fn header(name: &String) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        title { "DataRace - " (name) }
    }
} 

pub(crate) trait StaticHtml {
    fn generate_html(&self) -> Markup;
}

pub(crate) trait DynamicJs {
    fn generate_resize_js(&self) -> Markup;
    fn generate_update_js(&self) -> Markup;
}

impl StaticHtml for Dashboard {
    fn generate_html(&self) -> Markup {
        let mut names = vec![];
        for e in &self.elements {
            names = match e.gather_names(names) {
                Ok(list) => list,
                Err(err) => {
                    let out = format!("Failed to render Dashboard {} due to element name issues:\n{}", self.name, err);
                    error!("{}", &out);
                    return html!{
                        (header(&out))
                    };
                }
            }
        }

        html! {
            (header(&self.name))
            body {
                div id="BODY" style=(format!("position: absolute; left: 0px; top: 0px; width: {}px; height: {}px;", self.size_x, self.size_y)) {
                    @for item in &self.elements {
                        (item.generate_html())
                    }
                }
                div id="DISCO" style="position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; display: none; background-color: #F2F2F288;" {
                    div style="text-align: center; margin-top: 48vh; font-size: 2rem;" {
                        "Disconnected"
                    } 
                }
            }

            script src="/lib/socket.io.js" {}

            script src="/lib/datarace.dash.js" {}

            script {
                "var SOCKET = io();"

                "const DISCO = document.getElementById('DISCO');"
                "const BODY = document.getElementById('BODY');"
                @for n in names {
                    (format!("const {0} = document.getElementById('{0}');", n))
                }

                "let DATA = new Map();"
                "let SCALE = 0;"
                "console.log('Hello Everynya!');"

                "SOCKET.on('test', function(msg) {"
                    "console.log(msg);"
                "});"

                "SOCKET.on('require-auth', function() {"
                    "console.log('Server requested auth');"
                    (format!("SOCKET.emit('auth-dashboard', '{}');", &self.name))
                    "DISCO.style.display = 'none';"
                "});"


                "function resizeEvent() {"        
                    "{"
                        // We indent it to prevent name collisions
                        // as like rust js does masking when declaring a new variable with the same
                        // name in a lower scope using let/var/const.
                        // This prevents overriding const name of dashelements.
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

                        // Font scaling
                        (format!("fsize = {} * SCALE;", self.font_size))
                        (PreEscaped("document.documentElement.style.fontSize = fsize + \"px\";"))
                    "}"
                    

                    @for item in &self.elements {
                        (item.generate_resize_js())
                    }
                "}"
                
                "window.onresize = resizeEvent;"
                "resizeEvent();"

                "SOCKET.on('update', function(UP_ARR) {"
                    "const UPDATE = new Map(UP_ARR);"
                    // "console.log(UPDATE);"

                    (PreEscaped("UPDATE.forEach((value, key) => { if (value.ArrUpdate != null) {
                            let Arr = DATA.get(key);
                            const ArrUp = new Map(value.ArrUpdate);
                            ArrUp.forEach((value, key) => Arr.Arr[key] = value);
                        } else {
                            DATA.set(key, value);
                        }});"))
                    // "console.log(DATA);"

                    @for item in &self.elements {
                        (item.generate_update_js())
                    }
                "});"

                // Disconnect handler
                "SOCKET.on('disconnect', function() {"
                    "console.log('Lost connection');"
                    "DISCO.style.display = 'block';"
                "});"

                "function triggerAction(action) {"
                    // "console.log('Trigger Action');"
                    // "console.log(action);"
                    "SOCKET.emit('trigger_action', action);"
                "}"
            }
        }
        
    }
}

impl StaticHtml for DashElement {
    fn generate_html(&self) -> Markup {
        let name = if let Ok(n) = self.normalize_name() {
            n
        } else {
            return html!();
        };

        html! {
            div id=(name.as_str()) style=(format!("position: absolute; left:{}px; top:{}px; width:{}px; height:{}px;",
                self.x.get_static_value(), self.y.get_static_value(), self.size_x.get_static_value(), self.size_y.get_static_value())) {
                @match &self.element {
                    DashElementType::Square(color) => {
                        div style=(format!("width:100%;height:100%;background:{}", color)) {}
                    },
                    DashElementType::Folder(elements) => {
                        @for item in elements {
                            (item.generate_html())
                        }
                    },
                    DashElementType::Text(text) => {
                        (text.generate_html())
                    },
                    DashElementType::Button{ action, text } => {
                        button type="button" style="width:100%;height:100%" onClick=(match action {
                            Action::None => String::new(),
                            Action::Plugin(action_name) => {
                                if let Some(ac) = generate_web_action_handle(action_name.as_str()) { 
                                    format!("{} action: {} {}", "triggerAction({", ac, "})")
                                } else {
                                    String::new()
                                }
                            }
                        }) { (text.generate_html()) }
                    },
                    DashElementType::TextInput{ action, text } => {
                        input type="text" style=(format!("width:100%;height:100%;font-size:{}rem", 
                            text.font_size.as_ref().map_or(0.0, |p| p.get_static_value()))) 
                            onchange=(match action {
                                Action::None => String::new(),
                                Action::Plugin(action_name) => {
                                    if let Some(ac) = generate_web_action_handle(action_name.as_str()) { 
                                        format!("{} action: {}, param: {} Str: {}.firstElementChild.value {} {}", "triggerAction({", ac, "[{", name.as_str(), "}]", "})")
                                    } else {
                                        String::new()
                                    }
                                }
                            })
                            value=(text.text.get_static_value()) {}
                    }
                }
            }
        }
    }
}

impl DynamicJs for DashElement {
    fn generate_update_js(&self) -> Markup {
        let name = if let Ok(n) = self.normalize_name() {
            n
        } else {
            return html!();
        };

        html!{
            "{"
                // Handling visibility
                @if self.visible.is_computed() {
                    (PreEscaped(format!("if({})", self.visible.generate_read_js().into_string())))
                    "{"
                         (format!("{}.style.display = 'block';", name.as_str()))
                    "} else {"
                         (format!("{}.style.display = 'none';", name.as_str()))
                    "}"
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
                        // (PreEscaped(format!("console.log(DATA.get({}).Int == null);", serde_json::to_string(&text.get_property_handle()).unwrap())))
                        ((text, name.as_str()).generate_update_js())

                    },
                    DashElementType::Button{ action: _, text } => {
                        ((text, format!("{}.firstElementChild", name).as_str()).generate_update_js())
                    },
                    DashElementType::TextInput{ action: _, text } => {
                        @if text.text.is_computed() {
                            (PreEscaped(format!("{}.firstElementChild.value = {};", name, text.text.generate_read_js().into_string())))
                        }

                        @if let Some(fsize_prop) = &text.font_size {
                            @if fsize_prop.is_computed() {
                                (PreEscaped(format!("{}.firstElementChild.style.fontSize = \"{}rem\";", name, fsize_prop.generate_read_js().into_string())))
                            }
                        }
                    }
                } 
            "}"
        }
    }

    fn generate_resize_js(&self) -> Markup {
        let name = if let Ok(n) = self.normalize_name() {
            n
        } else {
            return html!();
        };

        html!{
            // We are in the resize function already,
            // we have access to the update scale value to apply to all dimensions
            "{"
                (PreEscaped(format!("let offset_x = {} * SCALE;", self.x.generate_read_js().into_string())))
                (PreEscaped(format!("let offset_y = {} * SCALE;", self.y.generate_read_js().into_string())))
                (PreEscaped(format!("let scale_x = {} * SCALE;", self.size_x.generate_read_js().into_string())))
                (PreEscaped(format!("let scale_y = {} * SCALE;", self.size_y.generate_read_js().into_string())))

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

impl StaticHtml for Text {
    fn generate_html(&self) -> Markup {
        let font_size_fix = if let Some(fsize_prop) = &self.font_size {
            fsize_prop.get_static_value()
        } else {
            1.0
        };

        html!{
            div style=(format!("font-size:{}rem", font_size_fix)) { (self.text.get_static_value()) }
        }
    }
}

impl DynamicJs for (&Text, &str) {
    fn generate_update_js(&self) -> Markup {
        let (text, name) = self;
        html!{
            @if text.text.is_computed() {
                (PreEscaped(format!("{}.firstElementChild.textContent = {};", name, text.text.generate_read_js().into_string())))
            }
            @if let Some(fsize_prop) = &text.font_size {
                @if fsize_prop.is_computed() {
                    (PreEscaped(format!("{}.firstElementChild.style.fontSize = \"{}rem\";", name, fsize_prop.generate_read_js().into_string())))
                }
            }
        }
    }

    fn generate_resize_js(&self) -> Markup {
        html!()
    }
}

/// Trait that provides the js for Dashboard Properties for reading and parsing the value into the
/// selected type
trait DynamicReadJs {
    fn generate_read_js(&self) -> PreEscaped<String>;
}

/// Generates the Data.get() call for the handle
trait HandleReadJs {
    fn generate_handle_js(&self) -> Option<String>;
}

impl DynamicReadJs for Property<bool> {
    fn generate_read_js(&self) -> PreEscaped<String> {
        let handle = match self.generate_handle_js() {
            Some(h) => h,
            None => {
                return if let Property::Fixed(val) = self {
                    PreEscaped(val.to_string())
                } else {
                    PreEscaped(self.get_static_value().to_string())
                };
            }
        };

        PreEscaped(match self {
            Property::Fixed(val) => {
                val.to_string()
            },
            Property::Computed(_) => {
                format!("read_bool({})", handle)
            },
            Property::Formated { source: _, formater } => {
                format!("parse_to_bool(pass_into({}, function(value) {{ {} }}))", handle, formater)
            },
            Property::Deref { source: _, index } => {
                format!("read_bool(read_arr({},{}))", handle, index.generate_read_js().into_string())
            }
        })
    }
}

impl DynamicReadJs for Property<i64> {
    fn generate_read_js(&self) -> PreEscaped<String> {
        let handle = match self.generate_handle_js() {
            Some(h) => h,
            None => {
                return if let Property::Fixed(val) = self {
                    PreEscaped(val.to_string())
                } else {
                    PreEscaped(self.get_static_value().to_string())
                };
            }
        };

        PreEscaped(match self {
            Property::Fixed(val) => {
                val.to_string()
            },
            Property::Computed(_) => {
                format!("read_int({})", handle)
            },
            Property::Formated { source: _, formater } => {
                format!("parse_to_int(pass_into({}, function(value) {{ {} }}))", handle, formater)
            },
            Property::Deref { source: _, index } => {
                format!("read_int(read_arr({},{}))", handle, index.generate_read_js().into_string())
            }
        })
    }
}

impl DynamicReadJs for Property<f64> {
    fn generate_read_js(&self) -> PreEscaped<String> {
        let handle = match self.generate_handle_js() {
            Some(h) => h,
            None => {
                return if let Property::Fixed(val) = self {
                    PreEscaped(val.to_string())
                } else {
                    PreEscaped(self.get_static_value().to_string())
                };
            }
        };

        PreEscaped(match self {
            Property::Fixed(val) => {
                val.to_string()
            },
            Property::Computed(_) => {
                format!("read_float({})", handle)
            },
            Property::Formated { source: _, formater } => {
                format!("parse_to_float(pass_into({}, function(value) {{ {} }}))", handle, formater)
            },
            Property::Deref { source: _, index } => {
                format!("read_float(read_arr({},{}))", handle, index.generate_read_js().into_string())
            }
        })
    }
}

impl DynamicReadJs for Property<String> {
    fn generate_read_js(&self) -> PreEscaped<String> {
        let handle = match self.generate_handle_js() {
            Some(h) => h,
            None => {
                return if let Property::Fixed(val) = self {
                    PreEscaped(val.to_string())
                } else {
                    PreEscaped(self.get_static_value().to_string())
                };
            }
        };

        PreEscaped(match self {
            Property::Fixed(val) => {
                val.to_string()
            },
            Property::Computed(_) => {
                format!("read_string({})", handle)
            },
            Property::Formated { source: _, formater } => {
                format!("pass_into({}, function(value) {{ {} }}).toString()", handle, formater)
            },
            Property::Deref { source: _, index } => {
                format!("read_string(read_arr({},{}))", handle, index.generate_read_js().into_string())
            }
        })
    }
}

impl<T> HandleReadJs for Property<T> {
    fn generate_handle_js(&self) -> Option<String> {
        let handle = PropertyHandle::new(self.get_property_handle()?.as_str())?;
        let web_handle: datarace_dashboard_spec::socket::PropertyHandle = handle.into();
        let serial = serde_json::to_string(&web_handle).ok()?;
        Some(format!("DATA.get({})", serial))
    }
}

fn generate_web_action_handle(name: &str) -> Option<String> {
    let handle = crate::ActionHandle::new(name)?;
    let web_handle: datarace_dashboard_spec::socket::ActionHandle = handle.into();
    Some(serde_json::to_string(&web_handle).ok()?)
}
