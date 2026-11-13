import "@mdi/font/css/materialdesignicons.css";
import "vuetify/styles";

import { createVuetify } from "vuetify";
import { createVueI18nAdapter } from "vuetify/locale/adapters/vue-i18n";
import type { I18n } from "vue-i18n";
import { useI18n } from "vue-i18n";

import { i18n } from "@/i18n";
import { initialTheme } from "@/theme";

export const vuetify = createVuetify({
  locale: {
    adapter: createVueI18nAdapter({
      // The adapter only needs the global composer; its public type pins the
      // locale generic to `string`, while this application narrows it.
      i18n: i18n as unknown as I18n<any, {}, {}, string, false>,
      useI18n,
    }),
  },
  theme: {
    defaultTheme: initialTheme(),
    themes: {
      light: {
        dark: false,
        colors: {
          background: "#f6f7fb",
          surface: "#ffffff",
          primary: "#3d5afe",
          secondary: "#5c6bc0",
          success: "#2e7d32",
          warning: "#ed6c02",
          error: "#c62828",
          info: "#0277bd",
        },
      },
      dark: {
        dark: true,
        colors: {
          background: "#12141a",
          surface: "#1b1e26",
          primary: "#8c9eff",
          secondary: "#9fa8da",
          success: "#81c784",
          warning: "#ffb74d",
          error: "#ef9a9a",
          info: "#4fc3f7",
        },
      },
    },
  },
  defaults: {
    VCard: { rounded: "lg" },
    VBtn: { rounded: "lg" },
    VTextField: {
      variant: "outlined",
      density: "compact",
      hideDetails: "auto",
    },
    VSelect: { variant: "outlined", density: "compact", hideDetails: "auto" },
    VAutocomplete: {
      variant: "outlined",
      density: "compact",
      hideDetails: "auto",
    },
    VDateInput: {
      variant: "outlined",
      density: "compact",
      hideDetails: "auto",
      prependIcon: null,
      prependInnerIcon: "$calendar",
    },
  },
});
