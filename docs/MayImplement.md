Some small features are up for consideration, ready to implement,
but to stop myself from getting bucked by scope creep more then necessary I have instead put them here:  
  
## Computing Value Container
A further few types in the Value Container, which instead of storing an Atomic value has a function which computes a value.  
Plugins can register/change to these "types", passing in a struct (without access to the internal types?) and function,
which will be passed into the function as pointer and will have a similar API to PluginHandle.  
This includes being able to subscribe to properties to read values, but this causes issues with subscriptions, which could change type after creation (like replacing a value with a computed value).  
Maybe need a channel, to which we could send a message, and just run try_read. But what if we find a message, how do we process it?
We do have to lock the function, so we can secure a mut lock on it. A single u32 lock should suffice, this should block everyone from executing,
while not stopping the others (but then again, we have to read an atomic on every read... yeah, so that gives up a lot of our gains from doing this).
Also if two grab a message each, then one after the other would aquire the lock and write their change.  
But this can cause an issue, because order could be mixed up. Also we can't guarantee that some other read that got through prior to lock has actually exited, could be stuck on a read of another property inside this.  
It would allow to mount simple scripts (writen in a scripting language like Lua) for transforming values right into a property.  
We also need to make sure there is a function for deallocating the void pointer in get_state

## Passthrough Value Container
We can take another Property, and just relay it.  
This is perfect for a universal GameManager, so it can take the property from any gamereader and relay it into one.  
We shallow_clone similar to subscription, and then set allow_edit inside the PropertyContainer to false (as we don't own it, we are just passing it through).  
We would need to be informed about the gamereader changing type of the original property, so we can change too.  
