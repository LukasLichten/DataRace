use log::error;
use maud::{html, Markup, PreEscaped, DOCTYPE};

use datarace_dashboard_spec::{Action, DashElement, DashElementType, Dashboard, Property, Text};

use crate::PropertyHandle;

pub(crate) trait WebDashboard {
    fn generate_dashboard(&self) -> Markup;
}

fn dashboard_header(name: &String) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="utf-8";
        title { "DataRace - " (name) }
    }
} 

impl WebDashboard for Dashboard {
    fn generate_dashboard(&self) -> Markup {
        let mut names = vec![];
        for e in &self.elements {
            names = match e.gather_names(names) {
                Ok(list) => list,
                Err(err) => {
                    let out = format!("Failed to render Dashboard {} due to element name issues:\n{}", self.name, err);
                    error!("{}", &out);
                    return html!{
                        (dashboard_header(&out))
                    };
                }
            }
        }

        html! {
            (dashboard_header(&self.name))

            style {
                @for n in &names {
                    (format!("div.{n} {}", "{"))
                        "position: absolute;"
                        "overflow: hidden;"
                    "}"
                }
            }

            body {
                div id="BODY" style=(format!("position: absolute; left: 0px; top: 0px; width: {}px; height: {}px; overflow: hidden", self.size_x, self.size_y)) {
                    @for item in &self.elements {
                        (item.generate_html())
                    }
                }
                div id="DISCO" style="position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; display: none; background-color: #F2F2F288;" {
                    div style="text-align: center; margin-top: 40vh; font-size: 2rem;" {
                        "Disconnected"
                    } 
                }
                div id="ERROR" style="position: absolute; left: 0px; bottom: 0px; width: 100%; height: 3.5rem; display: none; background-color: #F20000FF;" {
                    div style="text-align: center; margin-top: 1rem; font-size: 1.25rem;" {
                        ""
                    } 
                }
            }

            script src="/lib/socket.io.js" {}

            script src="/lib/datarace.dash.js" {}

            script {
                "var SOCKET = io();"
                "const ERROR = document.getElementById('ERROR');"

                "function MAIN() {"
                    "const DISCO = document.getElementById('DISCO');"
                    "const BODY = document.getElementById('BODY');"
                    "BODY.DR = {};"
                    @for item in &self.elements {
                        (item.generate_init_js("BODY"))
                    }

                    "let DATA = new Map();"
                    "let SCALE = 0;"
                    "console.log('Hello Everynya!');"

                    "SOCKET.on('test', function(msg) {"
                        "console.log(msg);"
                    "});"



                    "function resizeEvent() {"        
                            // We indent it to prevent name collisions
                            // as like rust js does masking when declaring a new variable with the same
                            // name in a lower scope using let/var/const.
                            // This prevents overriding const name of dashelements.

                            // "console.log('Resize Event: ' + window.innerWidth + '/' + window.innerHeight);"

                            (format!("let scale_to_w = window.innerWidth / {};", self.size_x))
                            (format!("let scale_to_h = window.innerHeight / {};", self.size_y))

                            (PreEscaped("if (scale_to_h < scale_to_w) {"))
                                // Window is wider then tall, so we are pillarboxing by offsetting the sides

                                // "console.log('Scaling Dashboard to Pillar Boxing (' + scale_to_h + 'x)');"

                                "SCALE = scale_to_h;"
                                (format!("let width = {} * SCALE;", self.size_x))
                                "let gap = (window.innerWidth - width)/2;"
                                "BODY.style.left = gap + 'px';"
                                "BODY.style.top = '0px';"
                                "BODY.style.width = width + 'px';"
                                "BODY.style.height = window.innerHeight + 'px';"
                            "} else {"
                                // Letterboxing instead

                                // "console.log('Scaling Dashboard to Letter Boxing (' + scale_to_w + 'x)');"

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
                        

                        @for item in &self.elements {
                            @if let Ok(n) = item.normalize_name() {
                                "try {"
                                    (format!("Resize_{0}(BODY.DR.{0}, DATA, SCALE);", n))
                                "} catch (error) {"
                                    // "if (error instanceof ReferenceError) {"
                                        "ERROR.firstElementChild.textContent = error;"
                                        "ERROR.style.display = 'block';"
                                    // "}"
                                    "console.log(error);"
                                "}"
                            }
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
                            @if let Ok(n) = item.normalize_name() {
                                "try {"
                                    (format!("Update_{0}(BODY.DR.{0}, DATA, SCALE);", n))
                                "} catch (error) {"
                                    // "if (error instanceof ReferenceError) {"
                                        "ERROR.firstElementChild.textContent = error;"
                                        "ERROR.style.display = 'block';"
                                    // "}"
                                    "console.log(error);"
                                "}"
                            }
                        }
                    "});"

                    // Disconnect handler
                    "SOCKET.on('disconnect', function() {"
                        "console.log('Lost connection');"
                        "DISCO.style.display = 'block';"
                    "});"

                "}"
                "MAIN();"

                "function triggerAction(action) {"
                    // "console.log('Trigger Action');"
                    // "console.log(action);"
                    "SOCKET.emit('trigger_action', action);"
                "}"

                "function triggerTextInputAction(id, action_handle) {"
                    
                "}"

                @for item in &self.elements {
                    (item.generate_resize_js())
                    (item.generate_update_js())
                }

                "SOCKET.on('require-auth', function() {"
                    "console.log('Server requested auth');"
                    (format!("SOCKET.emit('auth-dashboard', '{}');", &self.name))
                    "DISCO.style.display = 'none';"
                "});"

            }

            @for (name, f) in self.all_formatter_scripts() {
                script {
                    (PreEscaped(format!("function {name}(value) {}{f}{}", "{", "}")))
                }
            }
        }
        
    }

}

pub(crate) trait StaticHtml {
    fn generate_html(&self) -> Markup;
}

pub(crate) trait DynamicJs {
    fn generate_init_js(&self, parent: &str) -> Markup;
    fn generate_resize_js(&self) -> Markup;
    fn generate_update_js(&self) -> Markup;
}

impl StaticHtml for DashElement {
    fn generate_html(&self) -> Markup {
        let name = if let Ok(n) = self.normalize_name() {
            n
        } else {
            return html!();
        };

        html! {
            div class=(name.as_str()) style=(format!("left:{}px; top:{}px; width:{}px; height:{}px",
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
                        button type="button" style="width:100%;height:100%" onClick=(PreEscaped(match action {
                            Action::None => String::new(),
                            Action::Plugin(action_name) => {
                                if let Some(ac) = generate_web_action_handle(action_name.as_str()) { 
                                    format!("{} action: {} {}", "triggerAction({", ac.replace('\"', "'"), "})")
                                } else {
                                    String::new()
                                }
                            }
                        })) { (text.generate_html()) }
                    },
                    DashElementType::TextInput{ action: _, text } => {
                        input type="text" style=(format!("width:100%;height:100%;font-size:{}rem", 
                            text.font_size.as_ref().map_or(0.0, |p| p.get_static_value()))) 
                            value=(text.text.get_static_value()) {}
                    }
                }
            }
        }
    }
}

impl DynamicJs for DashElement {
    fn generate_init_js(&self, parent: &str) -> Markup {
        let name = if let Ok(n) = self.normalize_name() {
            n 
        } else {
            return html!();
        };
        
        html!(
            (format!("{1}.DR.{0} = {1}.getElementsByClassName('{0}')[0];", name.as_str(), parent))
            (format!("{1}.DR.{0}.DR = {2};", name.as_str(), parent, "{}"))

            
            @match &self.element {
                DashElementType::Folder(elements) => {
                    "{"
                        (format!("let {0} = {1}.DR.{0};", name.as_str(), parent))
                        @for item in elements {
                            (item.generate_init_js(name.as_str()))
                        }
                    "}"
                },
                DashElementType::TextInput{ action, text: _ } => {
                    "{"
                        (format!("let text_input = {1}.DR.{0}.firstElementChild;", name.as_str(), parent))
                        @match action {
                            Action::None => "",
                            Action::Plugin(action_name) => {
                                @if let Some(ac) = generate_web_action_handle(action_name.as_str()) { 
                                    (PreEscaped(format!("{0} action: {1}, param: {2} Str: text_input.value {3} {4}", 
                                        "text_input.addEventListener('change', function (){ triggerAction({", ac, "[{", "}]", "})});")))
                                }
                            }
                        }
                    "}"
                },
                _ => {}
            }
        )
    }

    fn generate_update_js(&self) -> Markup {
        let name = if let Ok(n) = self.normalize_name() {
            n
        } else {
            return html!();
        };


        html!{
            (format!("function Update_{}(element, DATA, SCALE)", name.as_str()))
            "{"
                // Handling visibility
                @if self.visible.is_computed() {
                    (PreEscaped(format!("if({})", self.visible.generate_read_js(name.as_str(), "visible").into_string())))
                    "{ element.style.display = 'block'; } else { element.style.display = 'none'; return; }"
                } @else {
                    (format!("element.style.display = {}",
                        match self.visible.get_static_value() {
                            true => "'block';",
                            false => "'none'; return;"
                        } ))
                }

                
                @if self.x.is_computed() || self.y.is_computed() || self.size_x.is_computed() || self.size_y.is_computed() {
                    // Trigger resize in case of an update, but only if dependent on it
                    (format!("Resize_{0}(element, DATA, SCALE);", name.as_str()))
                }

                
                // Updating internal value
                @match &self.element {
                    DashElementType::Square(color) => (format!("element.firstElementChild.style.background = '{}';", color)),
                    DashElementType::Folder(elements) => {
                        @for e in elements {
                            @if let Ok(n) = e.normalize_name() {
                                "try {"
                                    (format!("Update_{0}(element.DR.{0}, DATA, SCALE);", n))
                                "} catch (error) {"
                                    // "if (error instanceof ReferenceError) {"
                                        "ERROR.firstElementChild.textContent = error;"
                                        "ERROR.style.display = 'block';"
                                    // "}"
                                    "console.log(error);"
                                "}"
                            }
                        }
                    },
                    DashElementType::Text(text) => {
                        // (PreEscaped(format!("console.log(DATA.get({}).Int == null);", serde_json::to_string(&text.get_property_handle()).unwrap())))
                        ((text, "element", name.as_str()).generate_update_js())

                    },
                    DashElementType::Button{ action: _, text } => {
                        ((text, "element.firstElementChild", name.as_str()).generate_update_js())
                    },
                    DashElementType::TextInput{ action: _, text } => {
                        ((text, "element", name.as_str()).generate_update_js())
                        // @if text.text.is_computed() {
                        //     (PreEscaped(format!("element.firstElementChild.value = {};", text.text.generate_read_js(name.as_str(), "text").into_string())))
                        // }
                        //
                        // @if let Some(fsize_prop) = &text.font_size {
                        //     @if fsize_prop.is_computed() {
                        //         (PreEscaped(format!("element.firstElementChild.style.fontSize = \"{}rem\";", fsize_prop.generate_read_js(name.as_str(), "font_size").into_string())))
                        //     }
                        // }
                    }
                } 
            "}"

            @if let DashElementType::Folder(elements) = &self.element {
                @for e in elements {
                    (e.generate_update_js())
                }
            }
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
            (format!("function Resize_{}(element, DATA, SCALE)", name))
            "{"
                (PreEscaped(format!("let offset_x = {} * SCALE;", self.x.generate_read_js(name.as_str(), "x").into_string())))
                (PreEscaped(format!("let offset_y = {} * SCALE;", self.y.generate_read_js(name.as_str(), "y").into_string())))
                (PreEscaped(format!("let scale_x = {} * SCALE;", self.size_x.generate_read_js(name.as_str(), "size_x").into_string())))
                (PreEscaped(format!("let scale_y = {} * SCALE;", self.size_y.generate_read_js(name.as_str(), "size_y").into_string())))

                "element.style.left = offset_x + 'px';"
                "element.style.top = offset_y + 'px';"
                "element.style.width = scale_x + 'px';"
                "element.style.height = scale_y + 'px';"

                @if let DashElementType::Folder(elements) = &self.element {
                    @for e in elements {
                        @if let Ok(n) = e.normalize_name() {
                            (format!("Resize_{0}(element.DR.{0}, DATA, SCALE);", n))
                        }
                    }
                }
            "}"

            // Size in Folders does not constrain the content (except if I at some point implement % scaling)
            @if let DashElementType::Folder(elements) = &self.element {
                @for e in elements {
                    (e.generate_resize_js())
                }
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

impl DynamicJs for (&Text, &str, &str) {
    fn generate_init_js(&self, _: &str) -> Markup {
        html!()
    }

    fn generate_update_js(&self) -> Markup {
        let (text, callpath, name) = self;
        html!{
            @if text.text.is_computed() {
                (PreEscaped(format!("{}.firstElementChild.textContent = {};", callpath, text.text.generate_read_js(name, "text").into_string())))
                (PreEscaped(format!("{}.firstElementChild.value = {};", callpath, text.text.generate_read_js(name, "text").into_string())))
            }
            @if let Some(fsize_prop) = &text.font_size {
                @if fsize_prop.is_computed() {
                    (PreEscaped(format!("{}.firstElementChild.style.fontSize = \"{}rem\";", callpath, fsize_prop.generate_read_js(name, "font_size").into_string())))
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
    fn generate_read_js(&self, element_name: &str, field_name: &str) -> PreEscaped<String>;
}

/// Generates the Data.get() call for the handle
trait HandleReadJs {
    fn generate_handle_js(&self) -> Option<String>;
}

impl DynamicReadJs for Property<bool> {
    fn generate_read_js(&self, element_name: &str, field_name: &str) -> PreEscaped<String> {
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
            Property::Formated { source: _, formater: _ } => {
                format!("parse_to_bool({}({}))", self.get_formater_function_name(element_name, field_name), handle)
            },
            Property::Deref { source: _, index } => {
                format!("read_bool(read_arr({},{}))", handle, index.generate_read_js(self.get_formater_function_name(element_name, field_name).as_str(), "deref").into_string())
            }
        })
    }
}

impl DynamicReadJs for Property<i64> {
    fn generate_read_js(&self, element_name: &str, field_name: &str) -> PreEscaped<String> {
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
            Property::Formated { source: _, formater: _ } => {
                format!("parse_to_int({}({}))", self.get_formater_function_name(element_name, field_name), handle)
            },
            Property::Deref { source: _, index } => {
                format!("read_int(read_arr({},{}))", handle, index.generate_read_js(self.get_formater_function_name(element_name, field_name).as_str(), "deref").into_string())
            }
        })
    }
}

impl DynamicReadJs for Property<f64> {
    fn generate_read_js(&self, element_name: &str, field_name: &str) -> PreEscaped<String> {
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
            Property::Formated { source: _, formater: _ } => {
                format!("parse_to_float({}({}))", self.get_formater_function_name(element_name, field_name), handle)
            },
            Property::Deref { source: _, index } => {
                format!("read_float(read_arr({},{}))", handle, index.generate_read_js(self.get_formater_function_name(element_name, field_name).as_str(), "deref").into_string())
            }
        })
    }
}

impl DynamicReadJs for Property<String> {
    fn generate_read_js(&self, element_name: &str, field_name: &str) -> PreEscaped<String> {
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
            Property::Formated { source: _, formater: _ } => {
                format!("{}({}).toString()", self.get_formater_function_name(element_name, field_name), handle)
            },
            Property::Deref { source: _, index } => {
                format!("read_string(read_arr({},{}))", handle, index.generate_read_js(self.get_formater_function_name(element_name, field_name).as_str(), "deref").into_string())
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
