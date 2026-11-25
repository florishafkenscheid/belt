-- Test fixture: on map creation, leaves a character (like a disconnected player's) at spawn.
script.on_init(function()
  game.surfaces.nauvis.create_entity({ name = "character", position = { 0, 0 }, force = "player" })
end)
