-- Test that just accesses a field on self
local object = {x = 10}

function object:get_x()
  return self.x
end

print("The value of x is:", object:get_x())
return object:get_x() == 10
