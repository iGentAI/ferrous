-- Test basic upvalues and closures functionality

print("Testing closures and upvalues...")

-- Create a counter closure
function create_counter(start)
  local count = start
  return function(increment)
    count = count + increment
    return count
  end
end

-- Create two independent counters
local counter1 = create_counter(100)
local counter2 = create_counter(200)

-- Test counter1
print("Counter 1 results:")
print("  First call:", counter1(5))
print("  Second call:", counter1(10))

-- Test counter2
print("Counter 2 results:")
print("  First call:", counter2(7))
print("  Second call:", counter2(3))

-- Test that they maintain independent state
print("Verification:")
print("  Counter 1:", counter1(0))
print("  Counter 2:", counter2(0))

-- Return success if both counters work correctly
return (counter1(0) == 115 and counter2(0) == 210) and true or false
