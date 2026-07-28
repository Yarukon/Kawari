-- Windbite (BRD, ClassJob 23) - Level 30 weaponskill
-- Initial potency: 60 (wind-aspected)
-- DoT potency: 20 per tick, duration 45s
-- Applies status 129 (Windbite)
WINDBITE_STATUS = 129
DOT_DURATION = 45.0
INITIAL_POTENCY = 60
DOT_POTENCY = 20

function doAction(player, in_combo)
    effects = EffectsBuilder()
    -- Initial hit
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(INITIAL_POTENCY))
    -- Apply DoT (physical damage over time)
    effects:gain_dot_physical(WINDBITE_STATUS, 0, DOT_DURATION, DOT_POTENCY)

    return effects
end
