-- Apex Arrow (BRD, ClassJob 23) - Level 80 weaponskill (AoE)
-- Potency: 7 * Soul Voice (140 at SV=20 .. 700 at SV=100), scaled by Soul Voice spent.
-- Consumes all Soul Voice gauge (usable at SV >= 20).
-- At level 86+, grants Blast Arrow Ready status.
-- No AoE falloff: every target takes full potency (confirmed via ActionTransient 7.51).
BLAST_ARROW_READY_STATUS = 2692
BLAST_ARROW_READY_DURATION = 10.0
SOUL_VOICE_POTENCY_PER_POINT = 7

function doAction(player, in_combo)
    effects = EffectsBuilder()
    local soul_voice = player:bard_soul_voice()
    local potency = SOUL_VOICE_POTENCY_PER_POINT * soul_voice
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(potency))

    -- At level 86+, Apex Arrow grants Blast Arrow Ready
    if player:get_level() >= 86 then
        effects:gain_effect_self(BLAST_ARROW_READY_STATUS, 0, BLAST_ARROW_READY_DURATION)
    end
    effects:modify_gauge(0, -100)

    return effects
end
