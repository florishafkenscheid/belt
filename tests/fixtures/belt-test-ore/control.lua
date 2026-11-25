-- Test fixture: on map creation, covers spawn with a copper field, as natural ore would.
script.on_init(function()
  local surface = game.surfaces.nauvis
  for x = -20, 20 do
    for y = -20, 20 do
      surface.create_entity({ name = "copper-ore", position = { x + 0.5, y + 0.5 }, amount = 500 })
    end
  end
end)
