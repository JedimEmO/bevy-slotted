-- STAGE: data
-- EXPECT_ERROR: sandbox
-- The `slotted` table is frozen: assigning into it would give the next mod a
-- different API.
slotted.register_item = function() end
