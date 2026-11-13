<script setup lang="ts">
import { computed } from 'vue';

import { isoDate } from '@/utils/dates';

const props = withDefaults(defineProps<{
    modelValue: string
    label: string
    disabled?: boolean
    error?: boolean
    errorMessages?: string | string[]
}>(), {
    disabled: false,
    error: false,
    errorMessages: undefined,
})

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const date = computed<Date | null>({
    get: () => {
        const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(props.modelValue)
        if (!match) return null

        const value = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]))
        return value.getFullYear() === Number(match[1])
            && value.getMonth() === Number(match[2]) - 1
            && value.getDate() === Number(match[3])
            ? value
            : null
    },
    set: (value) => emit('update:modelValue', value ? isoDate(value) : ''),
})
</script>

<template>
    <VDateInput v-model="date" :label="label" :disabled="disabled" :error="error" :error-messages="errorMessages"
        clearable />
</template>
