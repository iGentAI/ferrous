-- This script tests the register window implementation of GetGlobal and SetGlobal

-- Define a function that both sets globals and uses them
function test_globals()
    -- Set multiple globals
    counter = 0
    message = "Global variable test"
    
    -- Function that increments a global
    function increment()
        counter = counter + 1
        return counter
    end
    
    -- Function that uses a global
    function get_message()
        return message
    end
    
    -- Set more globals to test register windows
    status = "active"
    data = {number = 42, text = message}
    
    -- Call functions that use globals
    local count = increment()
    local msg = get_message()
    
    return msg, count, status, data.number
end

-- Call our test function
return test_globals()
