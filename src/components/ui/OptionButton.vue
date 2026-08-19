<script setup lang="ts">
withDefaults(
  defineProps<{
    active?: boolean;
    disabled?: boolean;
    dimmed?: boolean;
    compact?: boolean;
  }>(),
  {
    active: false,
    disabled: false,
    dimmed: false,
    compact: false,
  },
);

defineEmits<{
  click: [];
  contextmenu: [event: MouseEvent];
}>();
</script>

<template>
  <button
    type="button"
    class="btn-skull min-w-0 flex-1 rounded-md border transition-colors"
    :class="[
      compact ? 'px-2 py-1.5 text-xs' : 'px-3 py-2.5',
      dimmed ? 'cursor-not-allowed opacity-35' : '',
      active
        ? 'is-active border-bp-accent text-bp-fg'
        : 'border-bp-border text-bp-muted hover:border-bp-accent/50 hover:text-bp-fg',
    ]"
    :disabled="disabled"
    @click="$emit('click')"
    @contextmenu.prevent="$emit('contextmenu', $event)"
  >
    <slot />
  </button>
</template>
