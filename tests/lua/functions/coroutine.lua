-- Coroutine Test
-- Tests coroutine creation, resumption, and yielding
-- Status: FAILING - Coroutines not implemented in current VM

local function test_coroutine()
  -- Basic coroutine that yields values in sequence
  local function counter(n)
    for i = 1, n do
      print("Yielding", i)
      coroutine.yield(i)
    end
    return "Done counting"
  end
  
  -- Create a coroutine
  local co = coroutine.create(counter)
  
  -- Resume it multiple times
  local results = {}
  print("Coroutine status:", coroutine.status(co))
  
  local success, value = coroutine.resume(co, 3)
  print("First resume:", success, value)
  if success then table.insert(results, value) end
  
  print("Coroutine status:", coroutine.status(co))
  success, value = coroutine.resume(co)
  print("Second resume:", success, value)
  if success then table.insert(results, value) end
  
  print("Coroutine status:", coroutine.status(co))
  success, value = coroutine.resume(co)
  print("Third resume:", success, value)
  if success then table.insert(results, value) end
  
  print("Coroutine status:", coroutine.status(co))
  success, value = coroutine.resume(co)
  print("Fourth resume:", success, value)
  if success then table.insert(results, value) end
  
  print("Final coroutine status:", coroutine.status(co))
  
  return results[1] == 1 and results[2] == 2 and results[3] == 3 and results[4] == "Done counting"
end

-- Test coroutine.wrap which provides a simplified interface
local function test_coroutine_wrap()
  local f = coroutine.wrap(function(n)
    for i = 1, n do
      coroutine.yield(i * 10)
    end
    return "Done wrapping"
  end)
  
  local results = {}
  
  -- Call the wrapped function repeatedly
  local value = f(3)
  print("First call result:", value)
  table.insert(results, value)
  
  value = f()
  print("Second call result:", value)
  table.insert(results, value)
  
  value = f()
  print("Third call result:", value)
  table.insert(results, value)
  
  value = f()
  print("Fourth call result:", value)
  table.insert(results, value)
  
  return results[1] == 10 and results[2] == 20 and results[3] == 30 and results[4] == "Done wrapping"
end

-- Run the tests
local success1, result1 = pcall(test_coroutine)
local success2, result2 = pcall(test_coroutine_wrap)

print("test_coroutine result:", success1, result1)
print("test_coroutine_wrap result:", success2, result2)

return success1 and result1 and success2 and result2