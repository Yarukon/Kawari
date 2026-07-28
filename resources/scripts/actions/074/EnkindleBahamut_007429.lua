-- 龙神迸发 / Enkindle Bahamut (Bahamut)
POTENCY = 1300

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_MAGIC, player.parameters:calc_magical_damage(POTENCY))

    return effects
end
