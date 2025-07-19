-- Comprehensive Arithmetic Operations Test
-- Tests all arithmetic operators with various type combinations
-- Includes edge cases, special values, and type coercion

local test_results = {}
local total_tests = 0
local passed_tests = 0

-- Helper function to run a test
local function test(name, actual, expected)
  total_tests = total_tests + 1
  -- Handle NaN comparisons specially
  local passed = false
  if actual ~= actual and expected ~= expected then
    -- Both are NaN
    passed = true
  elseif actual == expected then
    passed = true
  else
    -- Check if values are very close (for floating point precision)
    if type(actual) == "number" and type(expected) == "number" and 
       actual == actual and expected == expected then  -- not NaN
      local diff = math.abs(actual - expected)
      if diff < 1e-10 then
        passed = true
      end
    end
  end
  
  if passed then
    passed_tests = passed_tests + 1
    test_results[name] = true
  else
    test_results[name] = false
    print(string.format("FAILED: %s (expected %s, got %s)", name, tostring(expected), tostring(actual)))
  end
end

-- Create test values
local int_a = 10
local int_b = 3
local float_a = 10.0
float_b = 3.0
local mixed_a = 10
local mixed_b = 3.0
local zero_int = 0
local zero_float = 0.0
local neg_int = -5
local neg_float = -5.0
local large_int = 1000000
local large_float = 1000000.0
local small_float = 0.1
local inf = math.huge
local neg_inf = -math.huge
local nan = 0/0

-- String values that can be coerced
local str_int = "10"
local str_float = "3.5"
local str_neg = "-5"

print("=== Testing Addition (+) ===")

-- Basic integer addition
test("int + int", int_a + int_b, 13)
test("float + float", float_a + float_b, 13.0)
test("int + float", int_a + float_b, 13.0)
test("float + int", float_a + int_b, 13.0)

-- Zero addition
test("int + 0", int_a + zero_int, 10)
test("0 + int", zero_int + int_a, 10)
test("float + 0.0", float_a + zero_float, 10.0)
test("0.0 + float", zero_float + float_a, 10.0)

-- Negative number addition
test("pos + neg", int_a + neg_int, 5)
test("neg + pos", neg_int + int_a, 5)
test("neg + neg", neg_int + (-3), -8)

-- Large number addition
test("large + large", large_int + large_int, 2000000)
test("large float + int", large_float + 1, 1000001.0)

-- Small float addition
test("small + small", small_float + 0.2, 0.3)

-- Special values
test("inf + 1", inf + 1, inf)
test("inf + inf", inf + inf, inf)
test("inf + neg_inf", inf + neg_inf, nan)
test("neg_inf + neg_inf", neg_inf + neg_inf, neg_inf)
test("nan + 1", nan + 1, nan)
test("1 + nan", 1 + nan, nan)
test("nan + nan", nan + nan, nan)

-- String coercion
test("string int + int", str_int + 5, 15)
test("int + string int", 5 + str_int, 15)
test("string float + int", str_float + 5, 8.5)
test("string + string", str_int + str_float, 13.5)
test("string neg + int", str_neg + 10, 5)

print("\n=== Testing Subtraction (-) ===")

-- Basic subtraction
test("int - int", int_a - int_b, 7)
test("float - float", float_a - float_b, 7.0)
test("int - float", int_a - float_b, 7.0)
test("float - int", float_a - int_b, 7.0)

-- Zero subtraction
test("int - 0", int_a - zero_int, 10)
test("0 - int", zero_int - int_a, -10)
test("float - 0.0", float_a - zero_float, 10.0)

-- Negative number subtraction
test("pos - neg", int_a - neg_int, 15)
test("neg - pos", neg_int - int_a, -15)
test("neg - neg", neg_int - (-3), -2)

-- Large number subtraction
test("large - 1", large_int - 1, 999999)
test("1 - large", 1 - large_int, -999999)

-- Special values
test("inf - 1", inf - 1, inf)
test("1 - inf", 1 - inf, neg_inf)
test("inf - inf", inf - inf, nan)
test("neg_inf - neg_inf", neg_inf - neg_inf, nan)
test("nan - 1", nan - 1, nan)

-- String coercion
test("string int - int", str_int - 5, 5)
test("int - string int", 15 - str_int, 5)
test("string float - float", str_float - 0.5, 3.0)

print("\n=== Testing Multiplication (*) ===")

-- Basic multiplication
test("int * int", int_a * int_b, 30)
test("float * float", float_a * float_b, 30.0)
test("int * float", int_a * float_b, 30.0)
test("float * int", float_a * int_b, 30.0)

-- Zero multiplication
test("int * 0", int_a * zero_int, 0)
test("0 * int", zero_int * int_a, 0)
test("float * 0.0", float_a * zero_float, 0.0)

-- Negative number multiplication
test("pos * neg", int_a * neg_int, -50)
test("neg * pos", neg_int * int_a, -50)
test("neg * neg", neg_int * (-3), 15)

-- One multiplication
test("int * 1", int_a * 1, 10)
test("1 * int", 1 * int_a, 10)

-- Large number multiplication
test("large * 2", large_int * 2, 2000000)
test("large * 0.5", large_int * 0.5, 500000.0)

-- Special values
test("inf * 2", inf * 2, inf)
test("inf * -1", inf * (-1), neg_inf)
test("inf * 0", inf * 0, nan)
test("neg_inf * neg_inf", neg_inf * neg_inf, inf)
test("nan * 1", nan * 1, nan)
test("nan * 0", nan * 0, nan)

-- String coercion
test("string int * int", str_int * 5, 50)
test("int * string float", 2 * str_float, 7.0)

print("\n=== Testing Division (/) ===")

-- Basic division
test("int / int", int_a / int_b, 10/3)  -- approximately 3.33333...
test("float / float", float_a / float_b, 10.0/3.0)
test("int / float", int_a / float_b, 10/3.0)
test("float / int", float_a / int_b, 10.0/3)

-- Division by one
test("int / 1", int_a / 1, 10)
test("float / 1.0", float_a / 1.0, 10.0)

-- Division by negative
test("pos / neg", int_a / neg_int, -2)
test("neg / pos", neg_int / int_a, -0.5)
test("neg / neg", neg_int / (-5), 1)

-- Division by zero
test("1 / 0", 1 / zero_int, inf)
test("-1 / 0", -1 / zero_int, neg_inf)
test("0 / 0", zero_int / zero_int, nan)

-- Large number division
test("large / 1000", large_int / 1000, 1000)

-- Special values
test("inf / 2", inf / 2, inf)
test("2 / inf", 2 / inf, 0)
test("inf / inf", inf / inf, nan)
test("inf / neg_inf", inf / neg_inf, nan)
test("nan / 1", nan / 1, nan)
test("1 / nan", 1 / nan, nan)

-- String coercion
test("string int / int", str_int / 2, 5)
test("int / string float", 7 / str_float, 2.0)

print("\n=== Testing Modulo (%) ===")

-- Basic modulo (Lua uses floored division)
test("int % int", int_a % int_b, 1)  -- 10 % 3 = 1
test("float % float", float_a % float_b, 1.0)
test("int % float", int_a % float_b, 1.0)
test("float % int", float_a % int_b, 1.0)

-- Negative modulo (important edge cases for Lua's floored division)
test("pos % pos", 10 % 3, 1)       -- 10 - floor(10/3)*3 = 10 - 3*3 = 1
test("neg % pos", -10 % 3, 2)      -- -10 - floor(-10/3)*3 = -10 - (-4)*3 = 2
test("pos % neg", 10 % -3, -2)     -- 10 - floor(10/-3)*(-3) = 10 - (-4)*(-3) = -2
test("neg % neg", -10 % -3, -1)    -- -10 - floor(-10/-3)*(-3) = -10 - 3*(-3) = -1

-- Zero modulo
test("0 % int", zero_int % int_a, 0)
test("0 % float", zero_float % float_a, 0.0)

-- Modulo by one
test("int % 1", int_a % 1, 0)
test("float % 1.0", float_a % 1.0, 0.0)

-- Large number modulo
test("large % 1000", large_int % 1000, 0)
test("large % 1001", large_int % 1001, 1000)

-- Special values
test("inf % 2", inf % 2, nan)
test("2 % inf", 2 % inf, 2)
test("nan % 1", nan % 1, nan)
test("1 % nan", 1 % nan, nan)

-- String coercion
test("string int % int", str_int % 3, 1)
test("int % string int", 15 % str_int, 5)

-- Modulo by zero
test("1 % 0", 1 % zero_int, nan)
test("-1 % 0", -1 % zero_int, nan)

print("\n=== Testing Exponentiation (^) ===")

-- Basic exponentiation
test("int ^ int", 2 ^ 3, 8)
test("float ^ float", 2.0 ^ 3.0, 8.0)
test("int ^ float", 2 ^ 3.0, 8.0)
test("float ^ int", 2.0 ^ 3, 8.0)

-- Zero exponentiation
test("int ^ 0", int_a ^ 0, 1)
test("0 ^ int", 0 ^ int_a, 0)
test("0 ^ 0", 0 ^ 0, 1)  -- Lua defines 0^0 = 1

-- One exponentiation
test("int ^ 1", int_a ^ 1, 10)
test("1 ^ int", 1 ^ int_a, 1)

-- Negative exponentiation
test("pos ^ neg", 2 ^ (-2), 0.25)
test("neg ^ even", (-2) ^ 2, 4)
test("neg ^ odd", (-2) ^ 3, -8)

-- Fractional exponents
test("4 ^ 0.5", 4 ^ 0.5, 2.0)
test("8 ^ (1/3)", 8 ^ (1/3), 2.0)

-- Large exponents
test("2 ^ 10", 2 ^ 10, 1024)
test("10 ^ 6", 10 ^ 6, 1000000)

-- Special values
test("inf ^ 2", inf ^ 2, inf)
test("inf ^ -1", inf ^ (-1), 0)
test("inf ^ 0", inf ^ 0, 1)
test("2 ^ inf", 2 ^ inf, inf)
test("0.5 ^ inf", 0.5 ^ inf, 0)
test("1 ^ inf", 1 ^ inf, 1)
test("(-1) ^ inf", (-1) ^ inf, 1)  -- Lua returns 1 for this
test("nan ^ 1", nan ^ 1, nan)
test("1 ^ nan", 1 ^ nan, 1)  -- Lua returns 1 for 1^nan
test("nan ^ 0", nan ^ 0, 1)
test("0 ^ nan", 0 ^ nan, nan)

-- String coercion
test("string int ^ int", str_int ^ 2, 100)
test("int ^ string int", 2 ^ str_int, 1024)

print("\n=== Special Edge Cases ===")

-- Floating point precision
local a = 0.1 + 0.2
test("0.1 + 0.2", a, 0.3)  -- May have precision issues

-- Very large numbers
local huge1 = 9007199254740992  -- 2^53
local huge2 = 9007199254740992.0
test("2^53 int + 1", huge1 + 1, 9007199254740993)
test("2^53 float + 1", huge2 + 1, 9007199254740993.0)

-- Chain operations
test("1 + 2 * 3", 1 + 2 * 3, 7)  -- Precedence: * before +
test("(1 + 2) * 3", (1 + 2) * 3, 9)
test("2 ^ 3 ^ 2", 2 ^ 3 ^ 2, 512)  -- Right associative: 2^(3^2) = 2^9 = 512

-- Mixed operations with special values
test("inf - inf + 1", inf - inf + 1, nan)  -- nan + 1 = nan
test("0 * inf + 1", 0 * inf + 1, nan)      -- nan + 1 = nan

-- Zero comparisons
test("-0 + 0", -0.0 + 0.0, 0.0)
test("0 - 0", 0.0 - 0.0, 0.0)
test("-0 * -1", -0.0 * -1, 0.0)

print("\n=== Summary ===")
print(string.format("Total tests: %d", total_tests))
print(string.format("Passed: %d", passed_tests))
print(string.format("Failed: %d", total_tests - passed_tests))

-- Return test results
return {
  results = test_results,
  total = total_tests,
  passed = passed_tests,
  failed = total_tests - passed_tests,
  success = (total_tests == passed_tests)
}