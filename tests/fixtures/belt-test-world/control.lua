-- Test fixture: on map creation, builds what sanitize must report, away from spawn (enemies too). While the
-- map runs it also books production: 5 iron plates (normal) and 2 rare iron plates produced
-- and 2 normal consumed per tick, and 100 water produced per tick.
script.on_init(function()
  local surface = game.surfaces.nauvis
  surface.request_to_generate_chunks({ 300, 300 }, 2)
  surface.force_generate_chunk_requests()
  for _, entity in pairs(surface.find_entities_filtered({ area = { { 280, 280 }, { 330, 330 } } })) do
    if entity.type ~= "character" then entity.destroy() end
  end
  local tiles = {}
  for x = 280, 329 do for y = 280, 329 do tiles[#tiles + 1] = { name = "landfill", position = { x, y } } end end
  surface.set_tiles(tiles)
  local function place(name, x, y) surface.create_entity({ name = name, position = { x, y }, force = "player" }) end
  place("car", 290, 290)
  place("tank", 295, 290)
  place("rocket-silo", 310, 310)
  place("infinity-chest", 290, 300)
  place("infinity-pipe", 292, 300)
  place("electric-energy-interface", 294, 300)
  place("ee-infinity-chest", 297, 300)
  place("ee-infinity-helper", 299, 300)
  surface.pollute({ 300, 300 }, 500)
  -- Enemies, which a fresh map's starting area doesn't have.
  surface.create_entity({ name = "biter-spawner", position = { 320, 290 }, force = "enemy" })
  surface.create_entity({ name = "small-worm-turret", position = { 320, 300 }, force = "enemy" })
end)

script.on_event(defines.events.on_tick, function()
  local surface = game.surfaces.nauvis
  local items = game.forces.player.get_item_production_statistics(surface)
  items.on_flow({ name = "iron-plate", quality = "normal" }, 5)
  items.on_flow({ name = "iron-plate", quality = "rare" }, 2)
  items.on_flow({ name = "iron-plate", quality = "normal" }, -2)
  game.forces.player.get_fluid_production_statistics(surface).on_flow("water", 100)
end)
