-- Empyreal Arrow (BRD, ClassJob 23) - Level 54 ability
-- Potency: 260
-- Recast: 15s (CooldownGroup 3)
-- Does not share a recast timer with other weaponskills
POTENCY = 260

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(POTENCY))

    return effects
end
