-- Ladonsbite (BRD, ClassJob 23) - Level 82 weaponskill (cone AoE)
-- Potency: 140 (all targets in the cone; fan-out is server-side)
-- Upgrades from Quick Nock at level 82.
POTENCY = 140

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(POTENCY))

    return effects
end
