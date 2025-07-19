-- Type Coercion Test
-- Tests Lua's implicit and explicit type conversion behavior
-- Focuses on string-to-number and number-to-string conversions in various contexts

local test_results = {}
local total_tests = 0
local passed_tests = 0

-- Helper function to run a test
local function test(name, condition)
  total_tests = total_tests + 1
  if condition then
    passed_tests = passed_tests + 1
    test_results[name] = true
  else
    test_results[name] = false
    print("FAILED:", name)
  end
end

-- Helper function to test value equality (handles NaN)
local function test_equal(name, actual, expected)
  total_tests = total_tests + 1
  local passed = false
  
  -- Handle NaN comparisons
  if type(actual) == "number" and type(expected) == "number" and 
     actual ~= actual and expected ~= expected then
    passed = true
  elseif actual == expected then
    passed = true
  end
  
  if passed then
    passed_tests = passed_tests + 1
    test_results[name] = true
  else
    test_results[name] = false
    print(string.format("FAILED: %s (expected %s, got %s)", name, tostring(expected), tostring(actual)))
  end
end

-- Helper to safely test operations that might error
local function test_error(name, func)
  total_tests = total_tests + 1
  local ok, err = pcall(func)
  if not ok then
    passed_tests = passed_tests + 1
    test_results[name] = true
  else
    test_results[name] = false
    print("FAILED:", name, "(expected error but succeeded)")
  end
end

print("=== Testing String to Number Coercion in Arithmetic ===")

-- Basic string to number in arithmetic
test_equal("'5' + 3", "5" + 3, 8)
test_equal("3 + '5'", 3 + "5", 8)
test_equal("'5' + '3'", "5" + "3", 8)
test_equal("'10' - 3", "10" - 3, 7)
test_equal("'5' * '2'", "5" * "2", 10)
test_equal("'10' / '2'", "10" / "2", 5)
test_equal("'10' % '3'", "10" % "3", 1)
test_equal("'2' ^ '3'", "2" ^ "3", 8)

-- Floating point strings
test_equal("'3.14' + 1", "3.14" + 1, 4.14)
test_equal("'2.5' * 2", "2.5" * 2, 5.0)
test_equal("'10.5' - '0.5'", "10.5" - "0.5", 10.0)

-- Negative number strings
test_equal("'-5' + 10", "-5" + 10, 5)
test_equal("'-3.5' * 2", "-3.5" * 2, -7.0)
test_equal("10 + '-5'", 10 + "-5", 5)

-- Scientific notation strings
test_equal("'1e2' + 0", "1e2" + 0, 100)
test_equal("'1.5e2' + 0", "1.5e2" + 0, 150)
test_equal("'1e-2' + 0", "1e-2" + 0, 0.01)
test_equal("'-1e2' + 0", "-1e2" + 0, -100)
test_equal("'1E2' + 0", "1E2" + 0, 100)  -- Capital E

-- Hexadecimal notation strings
test_equal("'0x10' + 0", "0x10" + 0, 16)
test_equal("'0xFF' + 0", "0xFF" + 0, 255)
test_equal("'0xff' + 0", "0xff" + 0, 255)
test_equal("'0x10' + '0x20'", "0x10" + "0x20", 48)
test_equal("'-0x10' + 0", "-0x10" + 0, -16)

-- Whitespace handling
test_equal("'  5  ' + 0", "  5  " + 0, 5)
test_equal("'\\t10\\t' + 0", "\t10\t" + 0, 10)
test_equal("'\\n5\\n' + 0", "\n5\n" + 0, 5)
test_equal("'  3.14  ' + 0", "  3.14  " + 0, 3.14)
test_equal("'  0x10  ' + 0", "  0x10  " + 0, 16)
test_equal("'  -5  ' + 0", "  -5  " + 0, -5)

-- Special numeric strings
test_equal("'+5' + 0", "+5" + 0, 5)
test_equal("'+3.14' + 0", "+3.14" + 0, 3.14)
test_equal("'inf' + 0", "inf" + 0, math.huge)
test_equal("'-inf' + 0", "-inf" + 0, -math.huge)
test_equal("'+inf' + 0", "+inf" + 0, math.huge)

-- Edge case numeric strings
test_equal("'0' + 0", "0" + 0, 0)
test_equal("'-0' + 0", "-0" + 0, 0)
test_equal("'0.0' + 0", "0.0" + 0, 0)
test_equal("'.5' + 0", ".5" + 0, 0.5)
test_equal("'5.' + 0", "5." + 0, 5.0)

print("\n=== Testing Invalid String to Number Coercion ===")

-- These should cause errors in arithmetic operations
test_error("'' + 5", function() return "" + 5 end)
test_error("'hello' + 5", function() return "hello" + 5 end)
test_error("'5x' + 0", function() return "5x" + 0 end)
test_error("'x5' + 0", function() return "x5" + 0 end)
test_error("'5 5' + 0", function() return "5 5" + 0 end)
test_error("'0x' + 0", function() return "0x" + 0 end)
test_error("'0xG' + 0", function() return "0xG" + 0 end)
test_error("'nan' + 0", function() return "nan" + 0 end)
test_error("'NaN' + 0", function() return "NaN" + 0 end)

print("\n=== Testing tonumber() Function ===")

-- Basic conversions
test_equal("tonumber('5')", tonumber("5"), 5)
test_equal("tonumber('3.14')", tonumber("3.14"), 3.14)
test_equal("tonumber('-5')", tonumber("-5"), -5)
test_equal("tonumber('+5')", tonumber("+5"), 5)

-- Scientific notation
test_equal("tonumber('1e2')", tonumber("1e2"), 100)
test_equal("tonumber('1.5e-2')", tonumber("1.5e-2"), 0.015)

-- Hexadecimal
test_equal("tonumber('0x10')", tonumber("0x10"), 16)
test_equal("tonumber('0XFF')", tonumber("0XFF"), 255)
test_equal("tonumber('-0x10')", tonumber("-0x10"), -16)

-- Whitespace
test_equal("tonumber('  5  ')", tonumber("  5  "), 5)
test_equal("tonumber('\\t10\\t')", tonumber("\t10\t"), 10)

-- Special values
test_equal("tonumber('inf')", tonumber("inf"), math.huge)
test_equal("tonumber('-inf')", tonumber("-inf"), -math.huge)

-- Invalid conversions return nil
test("tonumber('') == nil", tonumber("") == nil)
test("tonumber('hello') == nil", tonumber("hello") == nil)
test("tonumber('5x') == nil", tonumber("5x") == nil)
test("tonumber('x5') == nil", tonumber("x5") == nil)
test("tonumber('nan') == nil", tonumber("nan") == nil)
test("tonumber('0x') == nil", tonumber("0x") == nil)

-- tonumber with base parameter
test_equal("tonumber('10', 10)", tonumber("10", 10), 10)
test_equal("tonumber('10', 16)", tonumber("10", 16), 16)
test_equal("tonumber('10', 8)", tonumber("10", 8), 8)
test_equal("tonumber('10', 2)", tonumber("10", 2), 2)
test_equal("tonumber('z', 36)", tonumber("z", 36), 35)
test_equal("tonumber('Z', 36)", tonumber("Z", 36), 35)
test_equal("tonumber('-10', 16)", tonumber("-10", 16), -16)
test("tonumber('0x10', 10) == nil", tonumber("0x10", 10) == nil)  -- 0x prefix only valid for base 16 (default)

-- Non-string arguments to tonumber
test_equal("tonumber(5)", tonumber(5), 5)
test_equal("tonumber(3.14)", tonumber(3.14), 3.14)
test("tonumber(nil) == nil", tonumber(nil) == nil)

print("\n=== Testing Number to String Coercion ===")

-- String concatenation forces number to string conversion
test("5 .. '' == '5'", 5 .. "" == "5")
test("3.14 .. '' == '3.14'", 3.14 .. "" == "3.14")
test("'value: ' .. 42 == 'value: 42'", "value: " .. 42 == "value: 42")
test("10 .. 20 == '1020'", 10 .. 20 == "1020")
test("-5 .. '' == '-5'", -5 .. "" == "-5")

-- Large numbers
local large = 1234567890
test("large .. ''", large .. "" == "1234567890")

-- Special values
test("math.huge .. ''", math.huge .. "" == "inf")
test("-math.huge .. ''", (-math.huge) .. "" == "-inf")
local nan = 0/0
test("(0/0) .. '' == '-nan' or 'nan'", 
     nan .. "" == "-nan" or nan .. "" == "nan" or nan .. "" == "-nan(ind)")  -- Platform differences

print("\n=== Testing tostring() Function ===")

-- Numbers
test("tostring(5) == '5'", tostring(5) == "5")
test("tostring(3.14) == '3.14'", tostring(3.14) == "3.14")
test("tostring(-5) == '-5'", tostring(-5) == "-5")
test("tostring(0) == '0'", tostring(0) == "0")

-- Special numeric values
test("tostring(math.huge) == 'inf'", tostring(math.huge) == "inf")
test("tostring(-math.huge) == '-inf'", tostring(-math.huge) == "-inf")

-- Other types
test("tostring(nil) == 'nil'", tostring(nil) == "nil")
test("tostring(true) == 'true'", tostring(true) == "true")
test("tostring(false) == 'false'", tostring(false) == "false")
test("tostring('hello') == 'hello'", tostring("hello") == "hello")

-- Tables and functions return their type and address (just check prefix)
local t = {}
local f = function() end
test("tostring(table) starts with 'table:'", string.sub(tostring(t), 1, 6) == "table:")
test("tostring(function) starts with 'function:'", string.sub(tostring(f), 1, 9) == "function:")

print("\n=== Testing Comparison Coercion ===")

-- String-number comparisons with coercion
test("5 == '5'", 5 == "5")
test("'5' == 5", "5" == 5)
test("3.14 == '3.14'", 3.14 == "3.14")
test("5 ~= '6'", 5 ~= "6")
test("'10' ~= 10.5", "10" ~= 10.5)

-- Comparisons with whitespace
test("5 == '  5  '", 5 == "  5  ")
test("'  5  ' == 5", "  5  " == 5)

-- Hex string comparisons
test("16 == '0x10'", 16 == "0x10")
test("'0x10' == 16", "0x10" == 16)

-- Ordering comparisons
test("5 < '10'", 5 < "10")
test("'5' < 10", "5" < 10)
test("'20' > 15", "20" > 15)
test("10 <= '10'", 10 <= "10")
test("'10' >= 10", "10" >= 10)

-- Invalid comparisons (different types without valid coercion)
test_error("5 < 'hello'", function() return 5 < "hello" end)
test_error("'hello' < 5", function() return "hello" < 5 end)

print("\n=== Testing Coercion in Other Contexts ===")

-- Unary minus
test_equal("-'5'", -"5", -5)
test_equal("-'3.14'", -"3.14", -3.14)
test_equal("-'0x10'", -"0x10", -16)
test_equal("-'  5  '", -"  5  ", -5)

-- Length operator doesn't coerce
test("#123 causes error", pcall(function() return #123 end) == false)

-- Boolean context (numbers and strings are always true, except nil and false)
test("if '0' then true", (function() if "0" then return true else return false end end)() == true)
test("if '' then true", (function() if "" then return true else return false end end)() == true)
test("if 0 then true", (function() if 0 then return true else return false end end)() == true)

-- Table indexing with number/string keys
local t = {[5] = "number", ["5"] = "string"}
test("t[5] ~= t['5']", t[5] ~= t["5"])
test("t[5] == 'number'", t[5] == "number")
test("t['5'] == 'string'", t["5"] == "string")

print("\n=== Testing Edge Cases ===")

-- Very large number strings
test_equal("'9007199254740992' + 0", "9007199254740992" + 0, 9007199254740992)

-- Leading zeros
test_equal("'007' + 0", "007" + 0, 7)
test_equal("'0.007' + 0", "0.007" + 0, 0.007)

-- Multiple signs
test_error("'++5' + 0", function() return "++5" + 0 end)
test_error("'--5' + 0", function() return "--5" + 0 end)
test_error("'+-5' + 0", function() return "+-5" + 0 end)

-- Partial hex strings
test_error("'0x' + 0", function() return "0x" + 0 end)
test_error("'0X' + 0", function() return "0X" + 0 end)

-- Case sensitivity
test_equal("'1e2' + 0", "1e2" + 0, 100)
test_equal("'1E2' + 0", "1E2" + 0, 100)
test_equal("'0xff' + 0", "0xff" + 0, 255)
test_equal("'0XFF' + 0", "0XFF" + 0, 255)

-- Concatenation with multiple values
test("'a' .. 1 .. 'b' == 'a1b'", "a" .. 1 .. "b" == "a1b")
test("1 .. 2 .. 3 == '123'", 1 .. 2 .. 3 == "123")

-- Mixed operations requiring coercion
test_equal("'5' + '5' .. ''", "5" + "5" .. "", "10")  -- Addition happens first
test("('5' .. '5') + 0 == 55", ("5" .. "5") + 0 == 55)  -- Concatenation first with parens

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