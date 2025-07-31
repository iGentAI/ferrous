-- Varargs Type Safety Test
-- This test specifically targets type confusion in varargs handling
-- where table construction and argument processing can result in type mismatches

-- Test 1: Basic varargs table construction
local function create_args_table(...)
  local args = {...}  -- This should create a proper table, not cause type confusion
  print("Args table type:", type(args))
  print("Args table length:", #args)
  return args
end

local args_table = create_args_table(1, 2, 3, "hello")
assert(type(args_table) == "table", "Varargs table construction failed")
assert(#args_table == 4, "Varargs table length incorrect")
assert(args_table[1] == 1, "Varargs table indexing failed")
assert(args_table[4] == "hello", "Varargs table string value failed")

-- Test 2: Varargs with ipairs iteration (this was failing)
local function test_varargs_ipairs(...)
  local args = {...}
  local sum = 0
  
  -- This should work with proper table type
  for i, v in ipairs(args) do
    print("Varargs ipairs:", i, "=", v)
    if type(v) == "number" then
      sum = sum + v
    end
  end
  
  return sum
end

local ipairs_result = test_varargs_ipairs(10, 20, 30, "skip")
print("Varargs ipairs sum:", ipairs_result)
assert(ipairs_result == 60, "Varargs ipairs processing failed")

-- Test 3: Varargs with select function
local function test_varargs_select(...)
  local count = select("#", ...)  -- Should return count correctly
  local second = select(2, ...)   -- Should return second argument
  
  print("Varargs count:", count)
  print("Second arg:", second)
  
  return count, second
end

local select_count, select_second = test_varargs_select("a", "b", "c", "d")
assert(select_count == 4, "Varargs select count failed")
assert(select_second == "b", "Varargs select indexing failed")

-- Test 4: Complex varargs with mixed operations
local function complex_varargs_test(...)
  local args = {...}
  local results = {}
  
  -- Multiple operations that could cause type confusion
  results.count = #args
  results.table_type = type(args)
  results.first_val = args[1]
  
  -- Iteration should work correctly
  for i, v in ipairs(args) do
    results["arg_" .. i] = v
  end
  
  return results
end

local complex_result = complex_varargs_test(100, 200, 300)
print("Complex varargs type:", complex_result.table_type)
print("Complex varargs count:", complex_result.count)
assert(complex_result.table_type == "table", "Complex varargs type confusion")
assert(complex_result.count == 3, "Complex varargs count failed")
assert(complex_result.arg_2 == 200, "Complex varargs indexing failed")

return "Varargs type safety test passed"