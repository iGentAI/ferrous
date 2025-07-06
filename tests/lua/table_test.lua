-- Table Operations Test Script
-- This test focuses on verifying all table operations and related opcodes

-- =======================================
-- Table Creation (NEWTABLE)
-- =======================================

-- Test 1: Empty table
local empty = {}
assert(type(empty) == "table")

-- Test 2: Array-style table
local array = {10, 20, 30, 40, 50}
assert(#array == 5)
assert(array[1] == 10)
assert(array[5] == 50)

-- Test 3: Dictionary-style table
local dict = {x = 1, y = 2, z = 3}
assert(dict.x == 1)
assert(dict.y == 2)
assert(dict.z == 3)

-- Test 4: Mixed table
local mixed = {
    100, 200, 300,
    name = "mixed",
    ["a key with spaces"] = "value"
}
assert(mixed[1] == 100)
assert(mixed.name == "mixed")
assert(mixed["a key with spaces"] == "value")

-- Test 5: Constructor with expressions
local a, b = 5, 7
local expr_table = {
    a + b,
    a * b,
    2 ^ a,
    [a] = b,
    [a + b] = a * b
}

assert(expr_table[1] == 12)  -- a + b
assert(expr_table[2] == 35)  -- a * b
assert(expr_table[3] == 32)  -- 2^5
assert(expr_table[5] == 7)   -- expr_table[a] == b
assert(expr_table[12] == 35) -- expr_table[a+b] == a*b

-- Test 6: Nested tables
local nested = {
    simple = {1, 2, 3},
    complex = {
        a = {
            inner = "value"
        }
    }
}

assert(nested.simple[2] == 2)
assert(nested.complex.a.inner == "value")

-- =======================================
-- Table Access (GETTABLE)
-- =======================================

-- Test 7: Direct indexing
local t7 = {x = 10, y = 20, z = 30}
assert(t7.x == 10)
assert(t7["y"] == 20)

-- Test 8: Variable indexing
local key = "z"
assert(t7[key] == 30)

-- Test 9: Expression indexing
assert(t7["x" .. "y"] == nil)  -- t7.xy doesn't exist
t7["x" .. "y"] = 100
assert(t7.xy == 100)

-- Test 10: Integer indexing
local arr = {5, 10, 15, 20, 25}
assert(arr[2] == 10)
assert(arr[4] == 20)

-- Test 11: Out of bounds access
assert(arr[0] == nil)
assert(arr[6] == nil)
assert(arr[-1] == nil)

-- Test 12: Mixed key types
local keys = {
    [1] = "numeric",
    [true] = "boolean",
    [false] = "boolean_false",
    [3.14] = "float",
    [{}] = "table"  -- table as a key
}

assert(keys[1] == "numeric")
assert(keys[true] == "boolean")
assert(keys[false] == "boolean_false")
assert(keys[3.14] == "float")

-- Test 13: Nested access
local deep = {
    level1 = {
        level2 = {
            level3 = {
                value = "deep"
            }
        }
    }
}

assert(deep.level1.level2.level3.value == "deep")
assert(deep["level1"]["level2"]["level3"]["value"] == "deep")
assert(deep["level1"].level2["level3"].value == "deep")

-- =======================================
-- Table Assignment (SETTABLE)
-- =======================================

-- Test 14: Basic assignment
local t14 = {}
t14.a = 1
t14.b = 2
t14["c"] = 3

assert(t14.a == 1)
assert(t14.b == 2)
assert(t14.c == 3)

-- Test 15: Overwriting values
t14.a = "new value"
assert(t14.a == "new value")

-- Test 16: Dynamic keys
local field = "dynamic"
t14[field] = "dynamic value"
assert(t14.dynamic == "dynamic value")

-- Test 17: Expression-based assignment
t14[1 + 2] = t14.a .. " extended"
assert(t14[3] == "new value extended")

-- Test 18: Nested assignment
local nested18 = {inner = {}}
nested18.inner.value = "nested value"
assert(nested18.inner.value == "nested value")

-- Test 19: Array-style assignment expansion
local arr19 = {}
for i = 1, 10 do
    arr19[i] = i * 10
end
assert(#arr19 == 10)
assert(arr19[5] == 50)

-- =======================================
-- Table Length (LEN)
-- =======================================

-- Test 20: Array length operator
local arr20 = {10, 20, 30, 40, 50}
assert(#arr20 == 5)

-- Test 21: Sparse arrays
local sparse = {
    [1] = "one",
    [3] = "three",
    [5] = "five"
}
-- Length is implementation-defined for sparse arrays, but often stops at first nil
assert(#sparse == 1 or #sparse == 5)

-- Test 22: Non-array tables
local non_array = {
    a = 1,
    b = 2,
    c = 3
}
assert(#non_array == 0) -- No array elements

-- Test 23: Mixed tables
local mixed23 = {
    10, 20, 30,
    a = 1,
    b = 2
}
assert(#mixed23 == 3)

-- =======================================
-- Table Iteration
-- =======================================

-- Test 24: pairs() iteration
local t24 = {a = 1, b = 2, c = 3}
local keys24 = {}
local values24 = {}

for k, v in pairs(t24) do
    keys24[k] = true
    values24[v] = true
end

assert(keys24.a and keys24.b and keys24.c)
assert(values24[1] and values24[2] and values24[3])

-- Test 25: ipairs() iteration
local t25 = {10, 20, 30, 40, 50}
local sum25 = 0

for i, v in ipairs(t25) do
    sum25 = sum25 + v
end

assert(sum25 == 150) -- 10+20+30+40+50

-- Test 26: Manual iteration using next()
local t26 = {x = 1, y = 2, z = 3}
local count26 = 0
local k, v = next(t26, nil)

while k do
    count26 = count26 + 1
    k, v = next(t26, k)
end

assert(count26 == 3)

-- =======================================
-- Table Methods
-- =======================================

-- Test 27: Object-oriented style
local counter = {
    value = 0,
    increment = function(self)
        self.value = self.value + 1
        return self.value
    end,
    decrement = function(self)
        self.value = self.value - 1
        return self.value
    end
}

assert(counter:increment() == 1)  -- Method call syntax (using SELF opcode)
assert(counter:increment() == 2)
assert(counter:decrement() == 1)
assert(counter.value == 1)

-- Test 28: Methods with upvalues
function create_counter(start)
    local self = {
        value = start or 0
    }
    
    function self:increment()
        self.value = self.value + 1
        return self.value
    end
    
    function self:decrement()
        self.value = self.value - 1
        return self.value
    end
    
    return self
end

local c = create_counter(10)
assert(c:increment() == 11)
assert(c:increment() == 12)
assert(c:decrement() == 11)

-- =======================================
-- Table Manipulation
-- =======================================

-- Test 29: Insert and remove
local t29 = {10, 20, 30}
table.insert(t29, 25)       -- Insert at end: {10, 20, 30, 25}
assert(#t29 == 4)
assert(t29[4] == 25)

table.insert(t29, 2, 15)    -- Insert at position: {10, 15, 20, 30, 25}
assert(#t29 == 5)
assert(t29[2] == 15)

local removed = table.remove(t29, 3)  -- Remove from position: {10, 15, 30, 25}
assert(removed == 20)
assert(#t29 == 4)
assert(t29[3] == 30)

-- Test 30: Concatenation
local t30 = {"a", "b", "c"}
local concat30 = table.concat(t30)    -- "abc"
assert(concat30 == "abc")

local concat_sep = table.concat(t30, "-")  -- "a-b-c"
assert(concat_sep == "a-b-c")

-- =======================================
-- Metatables
-- =======================================

-- Test 31: __index metamethod (table)
local t31_base = {shared = "base value"}
local t31 = {}
setmetatable(t31, {__index = t31_base})

assert(t31.shared == "base value")  -- Should use the metatable's __index

-- Test 32: __index metamethod (function)
local t32 = {}
setmetatable(t32, {
    __index = function(t, k)
        return k:upper()
    end
})

assert(t32.hello == "HELLO")  -- Should call the __index function

-- Test 33: __newindex metamethod
local t33_storage = {}
local t33 = {}
setmetatable(t33, {
    __newindex = function(t, k, v)
        t33_storage[k] = v * 2
    end
})

t33.value = 10
assert(t33.value == nil)        -- Direct table is unchanged
assert(t33_storage.value == 20) -- Value stored in storage, doubled

-- Test 34: __call metamethod
local t34 = {}
setmetatable(t34, {
    __call = function(t, arg1, arg2)
        return arg1 + arg2
    end
})

assert(t34(5, 7) == 12)  -- Calling a table like a function

-- Test 35: Arithmetic metamethods
local point_mt = {
    __add = function(a, b)
        return {x = a.x + b.x, y = a.y + b.y}
    end,
    
    __sub = function(a, b)
        return {x = a.x - b.x, y = a.y - b.y}
    end
}

local p1 = {x = 5, y = 10}
local p2 = {x = 2, y = 3}
setmetatable(p1, point_mt)
setmetatable(p2, point_mt)

local p3 = p1 + p2  -- {x=7, y=13}
local p4 = p1 - p2  -- {x=3, y=7}

assert(p3.x == 7 and p3.y == 13)
assert(p4.x == 3 and p4.y == 7)

-- =======================================
-- Advanced Table Scenarios
-- =======================================

-- Test 36: Tables as queues
local queue = {}

function queue:enqueue(item)
    table.insert(self, item)
end

function queue:dequeue()
    if #self > 0 then
        return table.remove(self, 1)
    end
    return nil
end

queue:enqueue("first")
queue:enqueue("second")
assert(queue:dequeue() == "first")
assert(queue:dequeue() == "second")
assert(queue:dequeue() == nil)

-- Test 37: Simplified object with table fields (avoid recursive functions)
function create_point(x, y)
    return {
        x = x or 0,
        y = y or 0,
        copy = function(self)
            return create_point(self.x, self.y)
        end
    }
end

local original = create_point(10, 20)
local copy = original:copy()

-- Modify original
original.x = 30
assert(copy.x == 10) -- Copy should remain unchanged

-- =======================================
-- Final Verification Report
-- =======================================

print("All table tests passed!")

return {
    creation_empty = type(empty) == "table",
    creation_array = array[3] == 30,
    creation_dict = dict.z == 3,
    creation_mixed = mixed.name == "mixed",
    creation_expr = expr_table[12] == 35,
    creation_nested = nested.complex.a.inner == "value",
    
    access_direct = t7.x == 10 and t7.y == 20,
    access_variable = t7[key] == 30,
    access_expr = t7.xy == 100,
    access_int = arr[4] == 20,
    access_nested = deep.level1.level2.level3.value == "deep",
    
    assignment_basic = t14.c == 3,
    assignment_dynamic = t14.dynamic == "dynamic value",
    assignment_expr = t14[3] == "new value extended",
    assignment_nested = nested18.inner.value == "nested value",
    
    length_array = #arr20 == 5,
    length_mixed = #mixed23 == 3,
    
    iteration_pairs = values24[1] and values24[2] and values24[3],
    iteration_ipairs = sum25 == 150,
    
    methods_self = counter.value == 1,
    closing_value = c.value == 11,
    
    insert_remove = #t29 == 4 and t29[3] == 30,
    string_concat = concat_sep == "a-b-c",
    
    metatable_index = t31.shared == "base value",
    metatable_index_func = t32.hello == "HELLO",
    metatable_newindex = t33_storage.value == 20,
    metatable_call = t34(5, 7) == 12,
    metatable_arithmetic = p3.x == 7 and p3.y == 13,
    
    advanced_queue = queue:dequeue() == nil,
    advanced_copy = copy.x == 10
}