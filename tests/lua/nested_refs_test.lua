-- Nested References Test Script
-- This test specifically focuses on deeply nested references and complex structures

-- =======================================
-- Nested Tables with Multiple Levels
-- =======================================

-- Create a deeply nested table structure
local deep = {
    level1 = {
        level2 = {
            level3 = {
                level4 = {
                    level5 = {
                        value = 42
                    }
                }
            }
        }
    }
}

-- Test direct access to deep nested value
assert(deep.level1.level2.level3.level4.level5.value == 42)

-- Test partial paths
local level2 = deep.level1.level2
assert(level2.level3.level4.level5.value == 42)

local level5 = deep.level1.level2.level3.level4.level5
assert(level5.value == 42)

-- =======================================
-- Self-Referential Tables
-- =======================================

-- Create a table that references itself
local self_ref = {name = "self_ref"}
self_ref.self = self_ref

-- Test self-reference
assert(self_ref.self.name == "self_ref")
assert(self_ref.self.self.self.name == "self_ref")

-- Create a loop of table references
local t1 = {name = "t1"}
local t2 = {name = "t2"}
local t3 = {name = "t3"}

t1.next = t2
t2.next = t3
t3.next = t1

-- Test reference loop
assert(t1.next.name == "t2")
assert(t1.next.next.name == "t3")
assert(t1.next.next.next.name == "t1") -- Back to first table

-- =======================================
-- Nested Functions with Shared Upvalues
-- =======================================

-- Create nested functions sharing upvalues
function create_nested_functions(initial)
    local shared = initial
    
    local function get_shared()
        return shared
    end
    
    local function set_shared(new_value)
        shared = new_value
    end
    
    local function modify()
        return function(value)
            shared = shared + value
            return shared
        end
    end
    
    return get_shared, set_shared, modify()
end

local get, set, modify = create_nested_functions(10)

-- Test shared upvalue access
assert(get() == 10)
set(20)
assert(get() == 20)
assert(modify(5) == 25) -- adds 5 to shared
assert(get() == 25)

-- =======================================
-- Complex Nested References
-- =======================================

-- Tables containing functions that reference the table
local complex = {
    value = 100,
    
    increment = function(self, amount)
        self.value = self.value + amount
        return self.value
    end,
    
    get_functions = function(self)
        return {
            add = function(x)
                return x + self.value
            end,
            
            multiply = function(x)
                return x * self.value
            end
        }
    end
}

-- Test self-reference in methods
assert(complex:increment(50) == 150)

-- Test functions that reference the table
local funcs = complex:get_functions()
assert(funcs.add(10) == 160)      -- 10 + 150
assert(funcs.multiply(2) == 300)  -- 2 * 150

-- =======================================
-- Tables as Upvalues in Closures
-- =======================================

function create_environment()
    local env = {
        variables = {},
        
        set = function(self, name, value)
            self.variables[name] = value
        
        end,
        
        get = function(self, name)
            return self.variables[name]
        end,
        
        create_getter = function(self, name)
            return function()
                return self.variables[name]
            end
        end,
        
        create_setter = function(self, name)
            return function(value)
                self.variables[name] = value
            end
        end
    }
    
    return env
end

local env = create_environment()
env:set("x", 42)
env:set("y", 100)

local get_x = env:create_getter("x")
local set_x = env:create_setter("x")
local get_y = env:create_getter("y")

assert(get_x() == 42)
set_x(99)
assert(get_x() == 99)
assert(env:get("x") == 99)
assert(get_y() == 100)

-- =======================================
-- Nested Tables with Function Values
-- =======================================

local operations = {
    math = {
        add = function(a, b) return a + b end,
        sub = function(a, b) return a - b end,
        mul = function(a, b) return a * b end,
        div = function(a, b) return a / b end
    },
    
    string = {
        concat = function(a, b) return a .. b end,
        upper = function(s) return s:upper() end,
        lower = function(s) return s:lower() end
    }
}

-- Test nested function access
assert(operations.math.add(5, 7) == 12)
assert(operations.math.mul(4, 10) == 40)
assert(operations.string.concat("hello", "world") == "helloworld")
assert(operations.string.upper("test") == "TEST")

-- =======================================
-- Dynamic Table Construction
-- =======================================

function create_matrix(rows, cols, initial)
    local matrix = {}
    
    for i = 1, rows do
        matrix[i] = {}
        for j = 1, cols do
            matrix[i][j] = initial(i, j)
        end
    end
    
    return matrix
end

-- Create a 3x3 identity matrix
local identity = create_matrix(3, 3, function(i, j)
    return i == j and 1 or 0
end)

assert(identity[1][1] == 1)
assert(identity[1][2] == 0)
assert(identity[2][2] == 1)
assert(identity[3][3] == 1)

-- Create a multiplication table 5x5
local mult_table = create_matrix(5, 5, function(i, j)
    return i * j
end)

assert(mult_table[2][3] == 6)  -- 2*3=6
assert(mult_table[4][5] == 20) -- 4*5=20

-- =======================================
-- Table Reference Updates
-- =======================================

local original = {value = "original"}
local alias = original

-- Test reference behavior
alias.value = "changed via alias"
assert(original.value == "changed via alias")

-- Function that updates a table
function update_table(t, new_value)
    t.value = new_value
end

update_table(original, "changed via function")
assert(alias.value == "changed via function")

-- =======================================
-- Final Verification Report
-- =======================================

print("All nested reference tests passed!")

return {
    deep_nesting = deep.level1.level2.level3.level4.level5.value == 42,
    self_reference = self_ref.self.self.self.name == "self_ref",
    reference_loop = t1.next.next.next.name == "t1",
    shared_upvalues = get() == 25,
    self_methods = complex.value == 150,
    nested_func_refs = funcs.multiply(2) == 300,
    env_closures = get_x() == 99 and get_y() == 100,
    nested_functions = operations.math.mul(4, 10) == 40,
    dynamic_tables = mult_table[4][5] == 20,
    reference_updates = alias.value == "changed via function"
}