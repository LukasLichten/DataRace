use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Dashboard {
    pub name: String,
    pub elements: Vec<DashElement>,
    pub size_x: i32,
    pub size_y: i32
}

impl Dashboard {
    pub fn list_properties(&self) -> HashSet<String> {
        let mut res = HashSet::<String>::new();

        for e in &self.elements {
            res.extend(e.list_properties());
        }

        res
    }
}

#[derive(Debug, Serialize, Deserialize)]
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
        
        if !name.chars().all(|x| x.is_ascii_digit() || x.is_ascii_lowercase() || x == '_') {
            return Err(format!("Unable to render dashboard: Name '{}' containes illegal characters (only ascii alphabet, numbers and _ permitted)", name));
        }

        return Ok(name);
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

}

#[derive(Debug, Serialize, Deserialize)]
pub enum DashElementType {
    Square(String),
    Text(Property<String>),
    Folder(Vec<DashElement>)
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
