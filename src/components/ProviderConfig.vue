<script lang="ts" setup>
import { NSpace } from "naive-ui";
import ProviderDetailForm from "./ProviderDetailForm.vue";
import ModelTable from "./ModelTable.vue";
import type { Provider } from "../libs/types";

defineProps<{ provider: Provider }>();
</script>

<template>
  <div class="container">
    <n-space vertical>
      <ProviderDetailForm :provider="provider" />
      <ModelTable :provider="provider" />
    </n-space>
  </div>
</template>

<style scoped>
.container {
  padding: 8px;
  box-sizing: border-box;
  width: 100%;
  height: 100%;
  overflow: auto;
}
</style>
