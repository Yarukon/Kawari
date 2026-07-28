-- 烈日龙神迸发 / Enkindle Solar Bahamut (Solar Bahamut)
POTENCY = 1500

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_MAGIC, player.parameters:calc_magical_damage(POTENCY))

    return effects
end
