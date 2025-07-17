-- Table Creation Test
-- Tests basic table creation

local empty_table = {}
print("Empty table:", empty_table)

local array_table = {1, 2, 3, 4, 5}
print("Array table length:", #array_table)

local record_table = {a = 1, b = 2, c = 3}
print("Record field:", record_table.a)

local mixed_table = {10, 20, x = "hello", y = "world"}
print("Mixed table array:", mixed_table[1])
print("Mixed table record:", mixed_table.x)

return type(empty_table) == "table" and
       #array_table == 5 and
       record_table.b == 2 and
       mixed_table[2] == 20 and
       mixed_table.y == "world"