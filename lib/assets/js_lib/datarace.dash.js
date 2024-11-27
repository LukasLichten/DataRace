// Library that handles parsing DataRace inputs for rendering on Dashboards
//
//

// Microseconds per second: 1s = 1000ms, 1ms = 1000us
const US_PER_SEC = 1000 * 1000;

function read_string(obj) {
	if (obj == null) {
		return "";
	}

	if (obj.Str != null) {
		return obj.Str;
	} else if (obj.Int != null) {
		return obj.Int.toString();
	} else if (obj.Float != null) {
		return obj.Float.toString();
	} else if (obj.Bool != null) {
		return obj.Bool.toString();
	} else if (obj.Dur != null) {
		return (obj.Dur / US_PER_SEC).toString() + "s";
	} else {
		// None or any other type
		return "";
	}
}

function read_int(obj) {
	if (obj == null) {
		return 0;
	}

	if (obj.Str != null) {
		var f = parseFloat(obj.Str);
		
		if (isNaN(f) 
			// || f === Infinity || f === -Infinity
			) {
			return 0;
		} else {
			return Math.round(f);
		}
	} else if (obj.Int != null) {
		return obj.Int;
	} else if (obj.Float != null) {
		if (isNaN(obj.Float) 
			// || obj.Float === Infinity || obj.Float === -Infinity
			) {
			return 0;
		} else {
			return Math.round(obj.Float);
		}
	} else if (obj.Bool != null) {
		return obj.Bool === true ? 1 : 0;
	} else if (obj.Dur != null) {
		return Math.round(obj.Dur / US_PER_SEC);
	} else {
		// None or any other type
		return 0;
	}
}

function read_float(obj) {
	if (obj == null) {
		return NaN;
	}

	if (obj.Str != null) {
		return parseFloat(obj.Str);
	} else if (obj.Int != null) {
		return obj.Int;
	} else if (obj.Float != null) {
		return obj.Float;
	} else if (obj.Bool != null) {
		return obj.Bool === true ? 1 : 0;
	} else if (obj.Dur != null) {
		return obj.Dur / US_PER_SEC;
	} else {
		// None or any other type
		return NaN;
	}
}

function read_bool(obj) {
	if (obj == null) {
		return false;
	}

	if (obj.Str != null) {
		var lowered = obj.Str.toLowerCase();
		
		if (lowered === "true") {
			return true;
		} else if (lowered === "false") {
			return false;
		}

		var num = parseFloat(lowered);
		return num === 1 ? true : false;
	} else if (obj.Int != null) {
		return obj.Int === 1 ? true : false;
	} else if (obj.Float != null) {
		return obj.Float === 1 ? true : false;
	} else if (obj.Bool != null) {
		return obj.Bool;
	} else if (obj.Dur != null) {
		return obj.Dur === 1 ? true : false;
	} else {
		// None or any other type
		return false;
	}
}


function read_dur(obj) {
	if (obj == null) {
		return NaN;
	}
	
	if (obj.Str != null) {
		var lowered = obj.Str.toLowerCase();
		
		if (lowered === "true") {
			return true;
		} else if (lowered === "false") {
			return false;
		}

		var num = parseFloat(lowered);
		return num === 1 ? true : false;
	} else if (obj.Int != null) {
		return obj.Int * US_PER_SEC;
	} else if (obj.Float != null) {
		return obj.Float * US_PER_SEC;
	} else if (obj.Bool != null) {
		return obj.Bool ? US_PER_SEC : 0;
	} else if (obj.Dur != null) {
		return obj.Dur;
	} else {
		// None or any other type
		return NaN;
	}
}

/// takes a value and a function, and then runs the value through the function
function pass_into(value, func) {
	return func(value);
}
