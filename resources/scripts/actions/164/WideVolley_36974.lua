-- Wide Volley (BRD, ClassJob 23) - Level 25 weaponskill (circle AoE)
-- Potency: 140 base; 220 ONLY under Barrage (128) — game data: "纷乱箭状态中威力：220".
-- Hawk's Eye (3861) merely enables the cast at 140; it does NOT raise the potency.
-- Gated to Hawk's Eye or Barrage (both procs consumed on use). Fan-out is server-side.
POTENCY = 140
BARRAGE_POTENCY = 220
HAWK_EYE_STATUS = 3861
BARRAGE_STATUS = 128

function doAction(player, in_combo)
    effects = EffectsBuilder()
    local potency = POTENCY
    if player:get_effect(BARRAGE_STATUS) then
        potency = BARRAGE_POTENCY
    end
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(potency))
    effects:lose_effect(HAWK_EYE_STATUS, 0)
    effects:lose_effect(BARRAGE_STATUS, 0)

    return effects
end
