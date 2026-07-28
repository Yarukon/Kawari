-- Venomous Bite (BRD, ClassJob 23) - Level 6 weaponskill
-- Initial potency: 100
-- DoT potency: 15 per tick, duration 45s
-- Applies status 124 (Venomous Bite)
VENOMOUS_BITE_STATUS = 124
DOT_DURATION = 45.0
INITIAL_POTENCY = 100
DOT_POTENCY = 15

function doAction(player, in_combo)
    effects = EffectsBuilder()
    -- Initial hit
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(INITIAL_POTENCY))
    -- Apply DoT (physical damage over time)
    effects:gain_dot_physical(VENOMOUS_BITE_STATUS, 0, DOT_DURATION, DOT_POTENCY)

    return effects
end
