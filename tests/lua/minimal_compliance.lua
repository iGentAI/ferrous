-- Minimal Lua compliance test that should work with current parser

-- Basic values
local a = 42
local b = "hello"
local c = true
local d = nil
local e = 3.14159

-- Arithmetic operations
local add_result = 10 + 5
local sub_result = 10 - 5
local mul_result = 10 * 5
local div_result = 10 / 5
local mod_result = 10 % 3
local pow_result = 2 ^ 3
local neg_result = -10

-- Logical operations
local and_result = true and true
local or_result = false or true
local not_result = not false

-- Comparisons
local eq_result = (1 == 1)
local neq_result = (1 ~= 2)
local lt_result = (1 < 2)
local lte_result = (2 <= 2)
local gt_result = (3 > 2)
local gte_result = (3 >= 2)

-- String operations
local concat_result = "hello" .. " world"
local len_result = #"hello"

-- Control flow
local if_result = 0
if true then
    if_result = 1
end

-- Simple loop
local sum = 0
local i = 1
while i <= 5 do
    sum = sum + i
    i = i + 1
end

-- Functions
local function add(x, y)
    return x + y
end

local function get_values()
    return 10, 20, 30
end

local v1, v2, v3 = get_values()

-- Simple table
local t = {}
t[1] = "one"
t[2] = "two"
t["key"] = "value"
t.field = "field_value"

-- Table constructor
local array = {1, 2, 3, 4, 5}
local dict = {x = 1, y = 2}
local mixed = {10, 20, name = "mixed"}

-- Method calls
local obj = {
    value = 0,
    increment = function(self)
        self.value = self.value + 1
        return self.value
    end
}

local inc_result = obj:increment()

print("All minimal compliance tests passed!")

-- Return a table with all test results
return {
    -- Basic values
    a = a,
    b = b,
    c = c,
    d = d,
    e = e,
    
    -- Arithmetic
    add_result = add_result,
    sub_result = sub_result,
    mul_result = mul_result,
    div_result = div_result,
    mod_result = mod_result,
    pow_result = pow_result,
    neg_result = neg_result,
    
    -- Logical
    and_result = and_result,
    or_result = or_result,
    not_result = not_result,
    
    -- Comparisons
    eq_result = eq_result,
    neq_result = neq_result,
    lt_result = lt_result,
    lte_result = lte_result,
    gt_result = gt_result,
    gte_result = gte_result,
    
    -- Strings
    concat_result = concat_result,
    len_result = len_result,
    
    -- Control flow
    if_result = if_result,
    sum = sum,
    
    -- Functions
    add_result = add(3, 4),
    v1 = v1,
    v2 = v2,
    v3 = v3,
    
    -- Tables
    t_1 = t[1],
    t_key = t["key"],
    t_field = t.field,
    array_3 = array[3],
    dict_y = dict.y,
    mixed_name = mixed.name,
    
    -- Methods
    inc_result = inc_result
}