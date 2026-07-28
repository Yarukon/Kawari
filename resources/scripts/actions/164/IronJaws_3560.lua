-- Iron Jaws (BRD, ClassJob 23) - Level 56 weaponskill
-- Potency: 100
-- Refreshes the duration of both Bard DoTs on the target
-- Requires at least one DoT to be active
POTENCY = 100
DOT_REFRESH_DURATION = 45.0

function doAction(player, in_combo)
    effects = EffectsBuilder()
    -- Damage hit
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(POTENCY))

    -- Refresh both DoTs by reapplying the statuses. The server refreshes same-status DoT slots.
    -- The script cannot read the enemy's DoT ids, so the caster's level picks which pair is active:
    -- the Lv64 upgrades (Caustic Bite / Stormbite) or their pre-Lv64 predecessors (Venomous Bite / Windbite).
    if player:get_level() >= 64 then
        -- Caustic Bite (1200) / Stormbite (1201)
        effects:gain_dot_physical(1200, 0, DOT_REFRESH_DURATION, 20)
        effects:gain_dot_physical(1201, 0, DOT_REFRESH_DURATION, 25)
    else
        -- Venomous Bite (124) / Windbite (129)
        effects:gain_dot_physical(124, 0, DOT_REFRESH_DURATION, 15)
        effects:gain_dot_physical(129, 0, DOT_REFRESH_DURATION, 20)
    end

    return effects
end
