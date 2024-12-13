# DataRace Dashboard Properties
Most Parts of a Dashboard Element is made up of Properties (such as visibility, size, position, content, etc.).  
  
Each Property can be any of these following:  
- `Fixed`
- `Computed`
- `Formated`
- `Deref`

*This functionalty is work and process, this document is not as detailed as it should be, and changes may not be documented immediatly*

## The Types 
### Fixed
Contains simply a value.  
The value can therefore not change during runtime.  
  
Using the incorrect type will result in the Dashboard not loading.

### Computed
Contains a string that is a PropertyHandle.  
The value is updated during Runtime, type converted (if necessary) and applied as is.

### Formated
Like `Computed` it contains a `source` that is a PropertyHandle string.  
But you also get the `formater` string, which contains javascript code.  
  
#### Formatter JS Function
The `formater` code is turned into a js function like this:
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
- Runtime errors will propagate upwards and halt the update (I may saveguard against this *eventually*)
- Syntax errors will stop the dashboard from loading at all
  - Make sure you have `;` at the end of EVERY statement
- Overriding critical variables (such as handles), which will result in breakages
  - Never omit `let/var/const` during declaration
  - You can also use this for "sandbox escape", allowing you to modify dashboard elements outside of what is usually possible
- Conflicts on the persitent storage of `document` (two elements accessing the same sub key)

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

### Deref
Used for processing arrays for PropertyHandle `source` at `index`.  
The `index` is a Property itself, therefore you can use `Fixed`, or any of the other computed (including another `Deref`)

## Example json
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
