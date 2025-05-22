# DataRace Socket Spec
This spec produces serializable structs (json) for Plugin connecting through the Websocket, 
but also Dashboards/Settings and how they are serialized.  

## Socket Spec



## Dashboard Spec
This spec produces serializable structs for Dashboards, but also Settings.  
  
It consists out of the following Elements:
- Dashboard: the overarching structs, contains all elements, name and size
- DashElement: Singular Element, although it can contain further Elements
- DashElementType: Defines what an Element is, and contains the required Properties for it
- Property: A Value, can be runtime defined and changing

Most of their behavior is documented via the given code. While you can manualy write the json for Dashboards,
or implement it in another language, due to it's WIP nature I won't provide much documentation for this right now 
(although the rust types should give you an idea).  
Large parts of this is documenting the bahaviors, some of which are implemented not in this crate but in the main DataRace lib 
(which does the conversion in html/js).  
These Dashboards are designed to be agnostic, so should be renderable in native UIs.

### DashElement
It is important to mention with DashElements that their `name` has to be unique (within a `dashboard`),
and that names are normalized to lower case. They can only contain ascii letters, numbers and `_`.
A name with only numbers is also not permittable.

### DashElementType
Similar to Property a rust enum is used here, which Serializes as such (example Text):
```
...
"element": {
    "Text": "Test"
}
...
```

Enum Varients with multiple named values (or contain a custom struct with multiple fields) are serialize as such:  
```
...
"element": {
    "Something": {
        "value1": {
            "Fixed": 5
        },
        ...
    }
}
...
```

### Property
Most Parts of a Dashboard Element is made up of Properties (such as visibility, size, position, content, etc.).  
They are not to be confused with DataRace Properties (created by plugins), although they can point to one for usage.
  
*This functionalty is work and process, this document is not as detailed as it should be, and changes may not be documented immediatly*

#### Fixed
Contains simply a value.  
The value can therefore not change during runtime.  
  
Using the incorrect type will result in the Dashboard not loading.

#### Computed
Contains a string that is a PropertyHandle.  
The value is updated during Runtime, type converted (if necessary) and applied as is.

#### Formated
Like `Computed` it contains a `source` that is a PropertyHandle string.  
But you also get the `formater` string, which contains javascript code.  

#### Deref
Used for processing arrays for PropertyHandle `source` at `index`.  
The `index` is a Property itself, therefore you can use `Fixed`, or any of the other computed (including another `Deref`)
  
### Formatter JS Function
Some Properties make use of a `formater`, in which case the contained code is turned into a js function like this:
```
function (value) {
    #formater code
}
```
The `value` is a DataRace Value Object, so you should make use of the conversion functions as needed:
```
read_string(obj)
read_int(obj)
read_float(obj)
read_dur(obj)
read_bool(obj)
read_arr(obj, index)
```
  
Formatter is run in the Browser, with the code invoked **every** time there is an **Update** (currently even if our value has **NOT changed**).  
As such you should avoid expensive computations, but it is ideal for reusing a property and 
keeping through that the need to send updates from DataRace to the Dashboard to a minimum.  
As it is a Browser it is also possible for you to use `document` for persitent storage.

But also you can break the entire Dashboard:
- Nothing prevents a sandbox escape via `document` and things like the `getElement` functions
- Conflicts on the persitent storage of `document` (two elements accessing the same sub key)
- Syntax and Runtime errors will prevent it's element (and all dependent to fail to update)
  - The dashboard will limp on in this state, but show an error
  - ReferenceErrors are usually caused by Syntax error in that function
  - Just make sure you have `;` or line break at the end of EVERY statement

Additional functions from `lib/assets/js_lib/datarace.dash.js` you can use for formatting:
```
// These functions only work on standard js datatypes, 
// and are mainly used by the callsite to concerse your return value into the correct type.
// But you may call them too, always possible there is some odd usecase for you.
parse_to_bool(value)
parse_to_int(value)
parse_to_float(value)

```
*More may come... eventually*


### Property Example Json
- Fixed *(Sets x-Position to 250)*:
```
"x": {
    "Fixed": 250
},
```
- Computed *(y-Position is determined by `sample_plugin.dash.pos_y`)*
```
"y": {
    "Computed": "sample_plugin.dash.pos_y"
},
```
- Formated *(visibility is determined by `sample_plugin.dashvis`, and then taken from an integer to a boolean through a modulo operation, careful, this can cause intense flashing)*
```
"visible": {
    "Formated": {
        "source": "sample_plugin.dashvis",
        "formater": "var i = read_int(value); return (i % 2 === 1);"
    }
},
```
- Deref *(text is determined by the second element of `sample_plugin.arr`)*
```
"Text": {
    "Deref": {
        "source": "sample_plugin.arr",
        "index": {
            "Fixed": 2
        }
    }
}
```
