import { createTestingPinia } from '@pinia/testing'
import { mount } from '@vue/test-utils'
import type { Component } from 'vue'

export function mountWithPlugins(component: Component, options: any = {}) {
  return mount(component, {
    global: {
      plugins: [createTestingPinia({ stubActions: false })],
      stubs: { transition: false },
      ...options.global,
    },
    ...options,
  })
}

export { createTestingPinia }
