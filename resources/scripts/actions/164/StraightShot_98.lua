-- Straight Shot (BRD, ClassJob 23) - Level 2 weaponskill
-- Potency: 200 (single target)
-- Gated to Hawk's Eye (3861) or Barrage (128); both procs are consumed on use.
-- (Barrage's single-target 3x multiplier is unimplemented, so no potency branch here.)
POTENCY = 200
HAWK_EYE_STATUS = 3861
BARRAGE_STATUS = 128

function doAction(player, in_combo)
    effects = EffectsBuilder()
    effects:damage(DAMAGE_TYPE_PIERCING, player.parameters:calc_physical_damage(POTENCY))
    effects:lose_effect(HAWK_EYE_STATUS, 0)
    effects:lose_effect(BARRAGE_STATUS, 0)

    return effects
end
