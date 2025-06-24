use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Hash)]
pub struct Dashboard {
    pub name: String,
    pub elements: Vec<DashElement>,
    pub size_x: i32,
    pub size_y: i32,
    pub font_size: i32
}

impl Dashboard {
    pub fn list_properties(&self) -> HashSet<String> {
        let mut res = HashSet::<String>::new();

        for e in &self.elements {
            res.extend(e.list_properties());
        }

        res
    }

    pub fn list_actions(&self) -> HashSet<String> {
        let mut res = HashSet::<String>::new();

        for e in &self.elements {
            e.list_actions(&mut res);
        }

        res
    }

    pub fn all_formatter_scripts(&self) -> Vec<(String, String)> {
        let mut list = Vec::new();
        for e in &self.elements {
            list.extend(e.all_formatter_scripts());
        }

        list
    }
}

#[derive(Debug, Serialize, Deserialize, Hash)]
pub struct DashElement {
    pub name: String,
    pub x: Property<i64>,
    pub y: Property<i64>,
    pub size_x: Property<i64>,
    pub size_y: Property<i64>,
    pub visible: Property<bool>,
    pub element: DashElementType,
}

impl DashElement {
    /// Names are reformated to lower case, but are also checked to insure requirements:
    /// ascii alphanumeric with additionally _
    pub fn normalize_name(&self) -> Result<String, String> {
        let name = self.name.to_lowercase();

        let mut non_number = 0;
        
        for x in name.chars() {
            if x.is_ascii_digit() {
                // This is valid, as long as there is somewhere a letter or underscor contained
            } else if x.is_ascii_lowercase() || x == '_' { 
                non_number += 1;
            } else {
                return Err(format!("Unable to render dashboard: Name '{}' containes illegal characters (only ascii alphabet, numbers and _ permitted)", name));
            }
        }

        if non_number > 0 {
            return Ok(name);
        } else {
            return Err(format!("Unable to render dashboard: Name '{}' is only numbers, requires at least one ascii letter or _", name));
        }
    }

    /// Gathers up the name of this element (and any potential sub elements)
    /// and insures there are no name collisions
    pub fn gather_names(&self, mut list: Vec<String>) -> Result<Vec<String>, String> {
        let name = match self.normalize_name() {
            Ok(n) => n,
            Err(e) => return Err(e)
        };

        if list.contains(&name) {
            return Err(format!("Unable to render dashboard: Unique Name violated with name '{}'", name));
        }

        list.push(name);

        if let DashElementType::Folder(elements) = &self.element {
            for e in elements {
                list = match e.gather_names(list) {
                    Ok(list) => list,
                    Err(err) => return Err(err)
                };
            }
        }

        Ok(list)
    }
    
    /// Returns a list of all properties used in scripts for this element
    /// and all elements contained in it
    fn list_properties(&self) -> HashSet<String> {
        let mut res = HashSet::<String>::new();

        match &self.element {
            DashElementType::Folder(elements) => {
                for e in elements {
                    res.extend(e.list_properties());
                }
            },
            DashElementType::Square(_) => {

            },
            DashElementType::Text(text) => {
                res.extend(text.list_properties());
            },
            DashElementType::Button { action:_, text } => {
                res.extend(text.list_properties());
            },
            DashElementType::TextInput { action:_, text } => {
                res.extend(text.list_properties());
            }
        }

        self.x.add_property_handle_to_collection(&mut res);
        self.y.add_property_handle_to_collection(&mut res);
        self.size_x.add_property_handle_to_collection(&mut res);
        self.size_y.add_property_handle_to_collection(&mut res);
        self.visible.add_property_handle_to_collection(&mut res);


        res
    }

    /// Appeneds the actions contained in this DashElement (if any)
    fn list_actions(&self, all: &mut HashSet<String>) {
        match &self.element {
            DashElementType::Folder(elements) => {
                for e in elements {
                    e.list_actions(all);
                }
            },
            DashElementType::Button { action, text: _ } => {
                action.add_action_name(all);
            },
            DashElementType::TextInput { action, text: _ } => {
                action.add_action_name(all);
            },
            _ => ()
        }
    }

    fn all_formatter_scripts(&self) -> Vec<(String, String)> {
        let mut list = Vec::new();
        let name = match self.normalize_name() {
            Ok(n) => n,
            Err(_) => return list
        };

        self.x.add_formater_functions(name.as_str(), "x", &mut list);
        self.y.add_formater_functions(name.as_str(), "y", &mut list);
        self.size_x.add_formater_functions(name.as_str(), "size_x", &mut list);
        self.size_y.add_formater_functions(name.as_str(), "size_y", &mut list);

        self.visible.add_formater_functions(name.as_str(), "visible", &mut list);

        match &self.element {
            DashElementType::Square(_) => {
            },
            DashElementType::Folder(elements) => {
                for e in elements {
                    list.extend(e.all_formatter_scripts());
                }
            },
            DashElementType::Text(text) => {
                text.all_formatter_scripts(name.as_str(), &mut list);
            },
            DashElementType::Button { action:_, text } => {
                text.all_formatter_scripts(name.as_str(), &mut list);
            },
            DashElementType::TextInput { action:_, text } => {
                text.all_formatter_scripts(name.as_str(), &mut list);
            }

        }
        
        list
    }

}

#[derive(Debug, Serialize, Deserialize, Hash)]
pub enum DashElementType {
    Square(String),
    Text(Text),
    Folder(Vec<DashElement>),
    Button{ action: Action, text: Text },
    TextInput{ action: Action, text: Text }
}

#[derive(Debug, Serialize, Deserialize, Hash)]
pub struct Text {
    pub text: Property<String>,
    pub font_size: Option<Property<f64>>,

}

impl Text {
    fn list_properties(&self) -> HashSet<String> {
        let mut res = HashSet::<String>::new();
        
        self.text.add_property_handle_to_collection(&mut res);
        if let Some(f) = self.font_size.as_ref() {
            f.add_property_handle_to_collection(&mut res);
        }

        res
    }

    fn all_formatter_scripts(&self, element_name: &str, list: &mut Vec<(String, String)>) {
        self.text.add_formater_functions(element_name, "text", list);
        
        if let Some(f) = self.font_size.as_ref() {
            f.add_formater_functions(element_name, "font_size", list);
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Property<T> {
    Fixed(T),
    Computed(String),

    // Formater function code has the following issues:
    // - Syntax errors result in Dashboard not running in general 
    // - Code can (likely) access variables, like Dashboard elements, and break the dashboard
    Formated{ source: String, formater: String },

    Deref{ source: String, index: Box<Property<i64>> }
}

impl<T> Property<T> {

    pub fn add_formater_functions(&self, element_name: &str, field_name: &str, list: &mut Vec<(String, String)>) {
        match self {
            Property::Formated { source: _, formater } => {
                list.push(
                (self.get_formater_function_name(element_name, field_name), formater.clone()))
            },
            Property::Deref { source: _, index } => {
                index.add_formater_functions(self.get_formater_function_name(element_name, field_name).as_str(), "deref", list);
            }
            _ => ()
        }
    }
    
    pub fn get_formater_function_name(&self, element_name: &str, field_name: &str) -> String {
        format!("{element_name}_F_{field_name}")
    }

    pub fn get_property_handle(&self) -> Option<String> {
        match self {
            Property::Fixed(_) => {
                None
            },
            Property::Computed(handle) => {
                Some(handle.clone())
            },
            Property::Formated { source, formater: _ } => {
                Some(source.clone())
            },
            Property::Deref { source, index: _ } => {
                Some(source.clone())
            }
        }

    }

    pub fn is_computed(&self) -> bool {
        match self {
            Property::Fixed(_) => false,
            _ => true,
        }
    }

    pub fn add_property_handle_to_collection(&self, set: &mut HashSet<String>) {
        if let Some(res) = self.get_property_handle() {
            set.insert(res);

            if let Property::Deref { source: _, index } = self {
                index.add_property_handle_to_collection(set);
            }
        }
    }
}

macro_rules! property_impl_hash {
    ($type: ident) => {
impl std::hash::Hash for Property<$type> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Property::Fixed(res) => res.hash(state),
            Property::Computed(func) => {
                func.hash(state);
            },
            Property::Formated { source, formater } => {
                source.hash(state);
                formater.hash(state);
            },
            Property::Deref { source, index } => {
                source.hash(state);
                index.hash(state);
            }
        }
    }
}
    };
}

property_impl_hash!(i64);
property_impl_hash!(bool);
property_impl_hash!(String);

// We have to do this seperate because rust being rust
impl std::hash::Hash for Property<f64> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Property::Fixed(res) => {
                res.to_bits().hash(state);
            },
            Property::Computed(func) => {
                func.hash(state);
            },
            Property::Formated { source, formater } => {
                source.hash(state);
                formater.hash(state);
            },
            Property::Deref { source, index } => {
                source.hash(state);
                index.hash(state);
            }
        }
    }
}

impl<T> Property<T> where T: Default + Clone {
    pub fn get_static_value(&self) -> T {
        match self {
            Property::Fixed(res) => res.clone(),
            Property::Computed(_) => {
                T::default()
            },
            Property::Formated { source: _, formater: _ } => {
                T::default()
            },
            Property::Deref { source: _, index: _ } => {
                T::default()
            }
        }
    }
}

#[derive(Debug,Clone,Serialize,Deserialize, Hash)]
pub enum Action {
    Plugin(String),
    None
}

impl Action {
    fn add_action_name(&self, all: &mut HashSet<String>) {
        match self {
            Self::Plugin(name) => {
                all.insert(name.clone());
            },
            Self::None => ()
        }
    }
}
