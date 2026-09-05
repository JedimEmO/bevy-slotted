-- STAGE: data
-- EXPECT: [
--   (type: "register_item", id: "test:n", def: {
--     "name": "test:n",
--     "int": 16,
--     "integral_float": 3,
--     "float": 0.5,
--     "negative": -2,
--     "yes": true,
--     "no": false
--   }),
-- ]
-- Luau has one number type: an integral value must come back as `Int` so a
-- `u32` field deserialises, and a fractional one as `Float`.
slotted.register_item("n", {
    int = 16,
    integral_float = 3.0,
    float = 0.5,
    negative = -2,
    yes = true,
    no = false,
})
