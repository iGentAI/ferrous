-- Comprehensive Lua Test Script
-- This script exercises all language features and opcodes to verify functionality

-- =======================================
-- Basic Values and Variables
-- =======================================
local nil_value = nil
local bool_true = true
local bool_false = false
local number_int = 42
local number_float = 3.14159
local string_simple = "hello world"
local string_escape = "quote: \" newline: \n tab: \t"
local string_long = [[
This is a long multi-line string
with several lines
of text.
]]

-- Test local and global variables
local local_var = 100
global_var = 200 

-- =======================================
-- Arithmetic Operations (ADD, SUB, MUL, DIV, MOD, POW, UNM)
-- =======================================
local add_result = 10 + 5        -- 15
local sub_result = 10 - 5        -- 5
local mul_result = 10 * 5        -- 50
local div_result = 10 / 5        -- 2
local mod_result = 10 % 3        -- 1
local pow_result = 2 ^ 3         -- 8
local unm_result = -10           -- -10

-- Precedence and mixed operations
local complex_result = 2 + 3 * 4 / 2 - 1  -- 2 + 6 - 1 = 7
local precedence_result = (2 + 3) * 4 / 2 -- 5 * 2 = 10

-- =======================================
-- Logical Operations (AND, OR, NOT)
-- =======================================

function assign(var, val)
    a = val
    return val
end

local and_result1 = true and true     -- true
local and_result2 = true and false    -- false
local and_result3 = false and true    -- false
local and_result4 = true and 42       -- 42 (last value if true)

local or_result1 = true or true       -- true (first value if true)
local or_result2 = true or false      -- true
local or_result3 = false or true      -- true
local or_result4 = false or 42        -- 42
local or_result5 = false or nil       -- nil

local not_result1 = not true          -- false
local not_result2 = not false         -- true
local not_result3 = not nil           -- true
local not_result4 = not 0             -- false (0 is truthy)

-- Short-circuit behavior
local a = 10
local short_circuit1 = false and assign(a, 20)  -- a should still be 10
local short_circuit2 = true or assign(a, 30)    -- a should still be 10
local short_circuit3 = true and assign(a, 40)   -- a should now be 40
local short_circuit4 = false or assign(a, 50)   -- a should now be 50

-- =======================================
-- String Operations (CONCAT, LEN)
-- =======================================
local concat_result = "hello" .. " " .. "world"  -- "hello world"
local len_string = #"hello"                      -- 5
local number_concat = "value: " .. 42            -- "value: 42"

-- =======================================
-- Table Operations
-- =======================================
-- Table constructor
local empty_table = {}
local array_table = {10, 20, 30, 40, 50}
local dict_table = {x = 1, y = 2, z = 3}
local mixed_table = {
    10, 20, 30,
    name = "mixed",
    ["key with spaces"] = 42
}

-- Table access (GETTABLE)
local array_value = array_table[2]            -- 20
local dict_value = dict_table.y               -- 2
local computed_key = "z"
local dynamic_value = dict_table[computed_key] -- 3
local dot_syntax = mixed_table.name           -- "mixed"
local bracket_syntax = mixed_table["name"]    -- "mixed"
local special_key = mixed_table["key with spaces"] -- 42

-- Table modification (SETTABLE)
array_table[3] = 300        -- Modify existing
array_table[6] = 60         -- Add new array element
dict_table.new_key = "new"  -- Add new field with dot syntax
dict_table["another"] = 42  -- Add new field with bracket syntax

-- Table length
local array_length = #array_table  -- 6 after adding element
local dict_length = #dict_table    -- 0 (non-array elements)

-- Nested tables
local matrix = {
    {1, 2, 3},
    {4, 5, 6},
    {7, 8, 9}
}
local nested_value = matrix[2][3]  -- 6

-- =======================================
-- Control Flow
-- =======================================

-- If-then-else (TEST, TESTSET, JMP)
local if_result = 0
if true then
    if_result = 10
elseif false then
    if_result = 20
else
    if_result = 30
end

-- Nested if
local nested_if_result = 0
if true then
    if false then
        nested_if_result = 1
    else
        nested_if_result = 2
    end
else
    nested_if_result = 3
end

-- While loop (TEST, JMP)
local i = 1
local while_sum = 0
while i <= 5 do
    while_sum = while_sum + i
    i = i + 1
end

-- Repeat-until loop (JMP, TEST)
local j = 5
local repeat_sum = 0
repeat
    repeat_sum = repeat_sum + j
    j = j - 1
until j == 0

-- For numeric loop (FORPREP, FORLOOP)
local for_sum = 0
for k = 1, 5 do
    for_sum = for_sum + k
end

-- For with step
local for_step_sum = 0
for k = 10, 1, -2 do
    for_step_sum = for_step_sum + k
end

-- For-in loop (TFORLOOP)
local for_in_sum = 0
for idx, value in ipairs(array_table) do
    for_in_sum = for_in_sum + value
end

-- Break statement
local break_sum = 0
for n = 1, 10 do
    if n > 5 then
        break
    end
    break_sum = break_sum + n
end

-- =======================================
-- Functions and Closures
-- =======================================

-- Basic function (CLOSURE, CALL, RETURN)
function add(a, b)
    return a + b
end

local add_call = add(5, 7)  -- 12

-- Local function
local function subtract(a, b)
    return a - b
end

local sub_call = subtract(10, 3)  -- 7

-- Anonymous function
local multiply = function(a, b)
    return a * b
end

local mul_call = multiply(4, 5)  -- 20

-- Multiple return values
function get_values()
    return 10, 20, 30
end

local val1, val2, val3 = get_values()

-- Functions with variable arguments
function sum_all(...)
    local args = {...}
    local total = 0
    for _, value in ipairs(args) do
        total = total + value
    end
    return total
end

local sum_result = sum_all(1, 2, 3, 4, 5)  -- 15

-- Closures and Upvalues (CLOSURE, GETUPVAL, SETUPVAL)
function make_counter(start)
    local count = start
    return function(increment)
        count = count + increment
        return count
    end
end

local counter = make_counter(10)
local first_count = counter(1)    -- 11
local second_count = counter(2)   -- 13

-- Nested functions with shared upvalues
function outer(x)
    local function inner(y)
        return x + y
    end
    return inner
end

local add5 = outer(5)
local added = add5(3)  -- 8

-- Mutual recursion
local is_even, is_odd

is_even = function(n)
    if n == 0 then return true end
    return is_odd(n - 1)
end

is_odd = function(n)
    if n == 0 then return false end
    return is_even(n - 1)
end

local even_test = is_even(4)  -- true
local odd_test = is_odd(3)    -- true

-- =======================================
-- Metatables and Metamethods
-- =======================================

-- Basic metatable with __index
local prototype = {value = 10}
local instance = {}
setmetatable(instance, {__index = prototype})

local meta_index_result = instance.value  -- 10

-- __index as a function
local dynamic_table = {}
setmetatable(dynamic_table, {
    __index = function(t, k)
        return k * 2
    end
})

local dynamic_result = dynamic_table[5]  -- 10

-- __newindex
local write_to = {}
local storage = {}
setmetatable(write_to, {
    __newindex = function(t, k, v)
        storage[k] = v * 2
    end
})

write_to.test = 5
local newindex_result = storage.test  -- 10

-- Arithmetic metamethods
local mt = {
    __add = function(a, b)
        return {value = a.value + b.value}
    end,
    __sub = function(a, b)
        return {value = a.value - b.value}
    end
}

local obj1 = {value = 10}
local obj2 = {value = 5}
setmetatable(obj1, mt)
setmetatable(obj2, mt)

local obj3 = obj1 + obj2
local metamethod_result = obj3.value  -- 15

-- =======================================
-- Advanced Table Operations
-- =======================================

-- Table traversal with pairs()
local table_keys = {}
local table_values = {}
local count = 1
for k, v in pairs(dict_table) do
    table_keys[count] = k
    table_values[count] = v
    count = count + 1
end

-- Table serialization (basic implementation)
function serialize_table(t)
    local result = "{"
    for k, v in pairs(t) do
        if type(k) == "string" then
            result = result .. "[\"" .. k .. "\"]="
        else
            result = result .. "[" .. k .. "]="
        end
        
        if type(v) == "table" then
            result = result .. serialize_table(v)
        elseif type(v) == "string" then
            result = result .. "\"" .. v .. "\""
        else
            result = result .. tostring(v)
        end
        result = result .. ","
    end
    return result .. "}"
end

local serialized = serialize_table({a = 1, b = 2, c = {d = 3}})

-- =======================================
-- Return Results
-- =======================================

-- Return a table with all test results
return {
    -- Basics
    nil_value = nil_value,
    bool_true = bool_true,
    bool_false = bool_false,
    number_int = number_int,
    number_float = number_float,
    string_simple = string_simple,
    string_escape = string_escape,
    
    -- Variables
    local_var = local_var,
    global_var = global_var,
    
    -- Arithmetic
    add_result = add_result,
    sub_result = sub_result,
    mul_result = mul_result,
    div_result = div_result,
    mod_result = mod_result,
    pow_result = pow_result,
    unm_result = unm_result,
    complex_result = complex_result,
    precedence_result = precedence_result,
    
    -- Logical
    and_result1 = and_result1,
    and_result2 = and_result2,
    and_result4 = and_result4,
    or_result1 = or_result1,
    or_result4 = or_result4,
    not_result1 = not_result1,
    not_result2 = not_result2,
    
    -- Strings
    concat_result = concat_result,
    len_string = len_string,
    
    -- Tables
    array_value = array_value,
    dict_value = dict_value,
    dot_syntax = dot_syntax,
    bracket_syntax = bracket_syntax,
    nested_value = nested_value,
    array_length = array_length,
    
    -- Control flow
    if_result = if_result,
    nested_if_result = nested_if_result,
    while_sum = while_sum,
    repeat_sum = repeat_sum,
    for_sum = for_sum,
    for_step_sum = for_step_sum,
    for_in_sum = for_in_sum,
    break_sum = break_sum,
    
    -- Functions
    add_call = add_call,
    sub_call = sub_call,
    mul_call = mul_call,
    val1 = val1, 
    val2 = val2,
    val3 = val3,
    sum_result = sum_result,
    
    -- Closures
    first_count = first_count,
    second_count = second_count,
    added = added,
    even_test = even_test,
    odd_test = odd_test,
    
    -- Metatables
    meta_index_result = meta_index_result,
    dynamic_result = dynamic_result,
    newindex_result = newindex_result,
    metamethod_result = metamethod_result
}