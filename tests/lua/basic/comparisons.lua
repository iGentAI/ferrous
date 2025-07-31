-- Comparison Operations Test
-- Tests all comparison operators with various type combinations
-- Includes special focus on number comparisons and edge cases

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

-- Create test values of different types
local test_nil = nil
local test_true = true
local test_false = false
local test_int = 42
local test_float = 42.0
local test_float_diff = 42.5
local test_string_num = "42"
local test_string = "hello"
local test_table1 = {}
local test_table2 = {}
local test_func1 = function() end
local test_func2 = function() end

-- Special number values
local test_zero_int = 0
local test_zero_float = 0.0
local test_neg_int = -5
local test_neg_float = -5.0
local test_inf = math.huge
local test_neg_inf = -math.huge
local test_nan = 0/0

print("=== Testing Equality (==) ===")

-- Nil comparisons
test("nil == nil", nil == nil)
test("nil == false", nil == false)
test("nil == 0", nil == 0)
test("nil == empty string", nil == "")
test("nil == table", nil == test_table1)

-- Boolean comparisons
test("true == true", true == true)
test("false == false", false == false)
test("true == false", true == false)
test("true == 1", true == 1)
test("false == 0", false == 0)

-- Number comparisons (critical for integer/float issues)
test("int == same int", 42 == 42)
test("float == same float", 42.0 == 42.0)
test("int == equivalent float", 42 == 42.0)
test("float == equivalent int", 42.0 == 42)
test("zero int == zero float", 0 == 0.0)
test("negative int == negative float", -5 == -5.0)
test("int == different float", 42 == 42.5)
test("inf == inf", math.huge == math.huge)
test("neg_inf == neg_inf", -math.huge == -math.huge)
test("inf == neg_inf", math.huge == -math.huge)
test("nan == nan", test_nan == test_nan)
test("nan == number", test_nan == 42)

-- String comparisons
test("string == same string", "hello" == "hello")
test("string == different string", "hello" == "world")
test("string number == same string number", "42" == "42")
test("string number == int", "42" == 42)
test("int == string number", 42 == "42")

-- Table comparisons (reference equality)
test("table == same table", test_table1 == test_table1)
test("table == different table", test_table1 == test_table2)

-- Function comparisons (reference equality)
test("function == same function", test_func1 == test_func1)
test("function == different function", test_func1 == test_func2)

print("\n=== Testing Inequality (~=) ===")

-- Basic inequality tests
test("nil ~= false", nil ~= false)
test("nil ~= 0", nil ~= 0)
test("true ~= false", true ~= false)
test("42 ~= 43", 42 ~= 43)
test("42 ~= 42.5", 42 ~= 42.5)
test("42 ~= 42.0", 42 ~= 42.0)
test("'hello' ~= 'world'", "hello" ~= "world")
test("table1 ~= table2", test_table1 ~= test_table2)
test("nan ~= nan", test_nan ~= test_nan)
test("inf ~= neg_inf", math.huge ~= -math.huge)

print("\n=== Testing Less Than (<) ===")

-- Number comparisons
test("1 < 2", 1 < 2)
test("2 < 1", 2 < 1)
test("42 < 42.5", 42 < 42.5)
test("42.0 < 42", 42.0 < 42)
test("42 < 42.0", 42 < 42.0)
test("-5 < 5", -5 < 5)
test("-5.0 < 5", -5.0 < 5)
test("0 < inf", 0 < math.huge)
test("neg_inf < 0", -math.huge < 0)
test("neg_inf < inf", -math.huge < math.huge)
test("nan < 0", test_nan < 0)
test("0 < nan", 0 < test_nan)

-- String comparisons
test("'a' < 'b'", "a" < "b")
test("'apple' < 'banana'", "apple" < "banana")
test("'10' < '2' (string)", "10" < "2")

-- Mixed type comparisons should error
-- test("number < string", 42 < "hello") -- This would error
-- test("nil < number", nil < 42) -- This would error

print("\n=== Testing Greater Than (>) ===")

-- Number comparisons
test("2 > 1", 2 > 1)
test("1 > 2", 1 > 2)
test("42.5 > 42", 42.5 > 42)
test("42 > 42.0", 42 > 42.0)
test("5 > -5", 5 > -5)
test("inf > 0", math.huge > 0)
test("0 > neg_inf", 0 > -math.huge)
test("nan > 0", test_nan > 0)

-- String comparisons
test("'b' > 'a'", "b" > "a")
test("'2' > '10' (string)", "2" > "10")

print("\n=== Testing Less Than or Equal (<=) ===")

-- Number comparisons
test("1 <= 2", 1 <= 2)
test("2 <= 2", 2 <= 2)
test("3 <= 2", 3 <= 2)
test("42 <= 42.0", 42 <= 42.0)
test("42.0 <= 42", 42.0 <= 42)
test("42 <= 42.5", 42 <= 42.5)
test("-5 <= -5.0", -5 <= -5.0)
test("neg_inf <= neg_inf", -math.huge <= -math.huge)
test("inf <= inf", math.huge <= math.huge)
test("nan <= nan", test_nan <= test_nan)

-- String comparisons
test("'a' <= 'a'", "a" <= "a")
test("'a' <= 'b'", "a" <= "b")

print("\n=== Testing Greater Than or Equal (>=) ===")

-- Number comparisons
test("2 >= 1", 2 >= 1)
test("2 >= 2", 2 >= 2)
test("1 >= 2", 1 >= 2)
test("42.0 >= 42", 42.0 >= 42)
test("42 >= 42.0", 42 >= 42.0)
test("42.5 >= 42", 42.5 >= 42)
test("-5.0 >= -5", -5.0 >= -5)
test("inf >= inf", math.huge >= math.huge)
test("nan >= 5", test_nan >= 5)

-- String comparisons
test("'b' >= 'a'", "b" >= "a")
test("'hello' >= 'hello'", "hello" >= "hello")

print("\n=== Special Number Cases ===")

-- Testing specific integer/float edge cases
local int1000 = 1000
local float1000 = 1000.0
test("1000 == 1000.0", int1000 == float1000)
test("not (1000 ~= 1000.0)", not (int1000 ~= float1000))
test("not (1000 < 1000.0)", not (int1000 < float1000))
test("not (1000 > 1000.0)", not (int1000 > float1000))
test("1000 <= 1000.0", int1000 <= float1000)
test("1000 >= 1000.0", int1000 >= float1000)

-- Large number tests
local large_int = 9007199254740992  -- 2^53
local large_float = 9007199254740992.0
test("large int == large float", large_int == large_float)

-- Very small differences
local a = 0.1 + 0.2
local b = 0.3
test("0.1 + 0.2 == 0.3", a == b)  -- This might fail due to floating point precision

-- Zero comparisons
test("-0.0 == 0.0", -0.0 == 0.0)
test("not (-0.0 < 0.0)", not (-0.0 < 0.0))

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