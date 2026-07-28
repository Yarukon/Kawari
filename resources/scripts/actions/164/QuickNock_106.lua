-- Quick Nock (BRD, ClassJob 23) - Level 18 weaponskill (cone AoE)
-- Potency: 110 (all targets in the cone; fan-out is server-side)
POTENCY = 110

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(POTENCY))

    return effects
end
