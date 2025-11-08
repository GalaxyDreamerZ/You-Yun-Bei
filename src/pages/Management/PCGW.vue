<script setup lang="ts">
// 功能：PCGW 索引查询页面
// 说明：提供输入框按名称或别名查询索引；展示返回的 GameInfo 及规则
// 依赖：tauri_specta 生成的 bindings，Element Plus 组件库，项目内 i18n 与通知

import { ref } from 'vue'
import { onMounted } from 'vue'
import { useNotification } from '~/composables/useNotification'
import { $t } from '~/i18n'
import { commands } from '~/bindings'

// 响应式状态
const queryName = ref('')
const isLoading = ref(false)
// 查询选项与结果集合
const fuzzy = ref(true)
const platform = ref<string | null>('windows')
const limit = ref<number>(20)
const results = ref<Array<{ info: any, score: number, matched_by: string }>>([])
const selected = ref<any | null>(null)
// 索引元信息（版本与条目数量）
const indexVersion = ref<string | null>(null)
const indexCount = ref<number>(0)
const isRefreshing = ref(false)
// 索引来源：已简化为仅本地 SQLite
// 不再支持 JSON 导入或远端刷新；前端仅提供元信息刷新与查询

const { showError, showSuccess } = useNotification()

// 环境标识（是否为 Tauri 桌面）
const isTauriEnv = ref(false)

/**
 * 检测是否运行在 Tauri 桌面环境
 *
 * - 说明：仅根据全局 `window.__TAURI__.core.invoke` 是否存在来判断，避免在浏览器模式中
 *   因为可导入 `@tauri-apps/api/core` 而误判，从而调用不可用的 `invoke` 导致错误。
 * - 兼容：不同版本 Tauri 的标志位可能不同，这里以 v2 的 `core.invoke` 为准；若未来需要兼容 v1，
 *   可同时检查 `window.__TAURI__?.tauri?.invoke`。
 */
async function detectTauriEnv() {
  const w: any = typeof window !== 'undefined' ? window : undefined
  isTauriEnv.value = !!(
    w && w.__TAURI__ && w.__TAURI__.core && typeof w.__TAURI__.core.invoke === 'function'
  )
}

/**
 * 查询 PCGW 索引（完整功能）
 *
 * - 输入：`queryName` 为游戏名称或别名；`fuzzy`/`platform`/`limit` 为查询选项
 * - 行为：调用后端命令 `pcgw_search`（前端绑定为 `pcgwSearch`），返回评分排序后的结果列表
 * - 兼容：在浏览器模式下返回空集合，并提示仅桌面模式支持
 * - 错误：将后端错误以通知形式展示
 */
async function handleQuery() {
  if (!queryName.value.trim()) {
    showError({ message: $t('pcgw.query_empty') })
    return
  }
  try {
    isLoading.value = true
    const opts = { fuzzy: fuzzy.value, platform: platform.value, limit: limit.value }
    const res = await commands.pcgwSearch(queryName.value.trim(), opts as any)
    if (res.status === 'ok') {
      const list = res.data
      results.value = list
      if (list.length > 0) {
        selected.value = list[0].info
        showSuccess({ message: $t('pcgw.query_ok') })
      } else {
        selected.value = null
        showError({ message: $t('misc.no_search_results') })
      }
    } else {
      showError({ message: res.error })
    }
  } catch (e) {
    console.error('pcgw_query failed:', e)
    showError({ message: $t('pcgw.query_failed') })
  } finally {
    isLoading.value = false
  }
}

/**
 * 刷新 PCGW 索引并更新页面状态
 *
 * - 行为：调用后端命令 `pcgw_refresh_index`（前端绑定为 `pcgwRefreshIndex`）获取元信息；在失败时给出通知
 */
async function handleRefreshIndex() {
  if (!isTauriEnv.value) {
    // 非桌面模式下不执行刷新，仅提示不可用
    showError({ message: $t('pcgw.desktop_only_hint') })
    return
  }
  try {
    isRefreshing.value = true
    const res = await commands.pcgwRefreshIndex()
    if (res.status === 'ok') {
      indexVersion.value = res.data.version ?? null
      indexCount.value = res.data.count ?? 0
      showSuccess({ message: $t('pcgw.refresh_ok') })
    } else {
      showError({ message: res.error })
    }
  } catch (e) {
    console.error('pcgw_refresh_index failed:', e)
    showError({ message: $t('pcgw.refresh_failed') })
  } finally {
    isRefreshing.value = false
  }
}

// 说明：导入相关功能已移除（逻辑改为仅从本地 SQLite 读取）

/**
 * 初始化：桌面模式自动获取索引元信息
 */
onMounted(() => {
  // 先检测环境，再据此自动刷新索引
  // eslint-disable-next-line @typescript-eslint/no-floating-promises
  (async () => {
    await detectTauriEnv()
    if (isTauriEnv.value) {
      await handleRefreshIndex()
    }
  })()
})
</script>

<template>
  <div class="pcgw-container">
    <h2>{{ $t('pcgw.title') }}</h2>
    <el-form label-position="top">
      <el-form-item :label="$t('pcgw.query_label')">
        <el-input v-model="queryName" :placeholder="$t('pcgw.query_placeholder')" />
      </el-form-item>
      <!-- 索引状态与刷新按钮 -->
      <div class="index-status-row">
        <el-tag type="info" class="mr-8">
          {{ $t('pcgw.index_version') }}: {{ indexVersion ?? '-' }}
        </el-tag>
        <el-tag type="success" class="mr-8">
          {{ $t('pcgw.index_count') }}: {{ indexCount }}
        </el-tag>
        <el-button type="default" :loading="isRefreshing" @click="handleRefreshIndex">
          {{ $t('pcgw.refresh_action') }}
        </el-button>
        <el-alert type="info" class="ml-8" :closable="false" show-icon
          :title="$t('pcgw.sqlite_only_hint')"
        />
      </div>
      <div class="options-row">
        <el-form-item :label="$t('pcgw.fuzzy_label')">
          <el-switch v-model="fuzzy" />
        </el-form-item>
        <el-form-item :label="$t('pcgw.platform_label')" class="ml-16">
          <el-select v-model="platform" placeholder="{{ $t('pcgw.platform_placeholder') }}" style="width: 180px;">
            <el-option label="windows" value="windows" />
            <el-option label="macos" value="macos" />
            <el-option label="linux" value="linux" />
            <el-option :label="$t('pcgw.platform_all')" :value="null" />
          </el-select>
        </el-form-item>
        <el-form-item :label="$t('pcgw.limit_label')" class="ml-16">
          <el-input-number v-model="limit" :min="1" :max="100" />
        </el-form-item>
      </div>
      <el-alert v-if="!isTauriEnv" type="warning" :title="$t('pcgw.desktop_only_hint')" show-icon class="mb-12" />
      <el-button type="primary" :disabled="!isTauriEnv" :loading="isLoading" @click="handleQuery">
        {{ $t('pcgw.query_action') }}
      </el-button>
    </el-form>

    <!-- 查询结果列表 -->
    <div v-if="results.length > 0" class="pcgw-list mt-16">
      <el-table :data="results" style="width: 100%" @row-click="row => (selected = row.info)">
        <el-table-column :label="$t('misc.search')" width="40">
          <template #default="scope"><span>#</span></template>
        </el-table-column>
        <el-table-column prop="info.name" :label="$t('addgame.game_name')" />
        <el-table-column prop="matched_by" :label="$t('pcgw.matched_by')" width="120" />
        <el-table-column prop="score" :label="$t('pcgw.score')" width="120" />
        <el-table-column :label="'PCGW'" width="160">
          <template #default="scope">
            <el-tag v-if="scope.row.info.pcgw_id" type="success">{{ scope.row.info.pcgw_id }}</el-tag>
            <span v-else>-</span>
          </template>
        </el-table-column>
      </el-table>
    </div>

    <!-- 选中结果详情展示 -->
    <div v-if="selected" class="pcgw-result">
      <el-card>
        <template #header>
          <div class="card-header">
            <span>{{ selected!.name }}</span>
            <el-tag v-if="selected!.pcgw_id" type="success" class="ml-8">PCGW: {{ selected!.pcgw_id }}</el-tag>
          </div>
        </template>

        <div class="meta">
          <div class="meta-row">
            <span class="meta-label">{{ $t('pcgw.aliases') }}:</span>
            <span class="meta-value">{{ selected!.aliases.join(', ') || '-' }}</span>
          </div>
        </div>

        <h3 class="section-title">{{ $t('pcgw.save_rules') }}</h3>
        <el-table :data="selected!.save_rules" style="width: 100%">
          <el-table-column prop="id" :label="$t('pcgw.col_id')" width="200" />
          <el-table-column prop="description" :label="$t('pcgw.col_desc')" width="260" />
          <el-table-column prop="path_template" :label="$t('pcgw.col_path')" />
          <el-table-column prop="platforms" :label="$t('pcgw.col_platforms')" width="160">
            <template #default="scope">
              <el-tag v-for="p in scope.row.platforms" :key="p" class="mr-4">{{ p }}</el-tag>
            </template>
          </el-table-column>
          <el-table-column prop="confidence" :label="$t('pcgw.col_confidence')" width="140" />
        </el-table>

        <h3 class="section-title mt-16">{{ $t('pcgw.install_rules') }}</h3>
        <el-table :data="selected!.install_rules" style="width: 100%">
          <el-table-column prop="id" :label="$t('pcgw.col_id')" width="200" />
          <el-table-column prop="description" :label="$t('pcgw.col_desc')" width="260" />
          <el-table-column prop="patterns" :label="$t('pcgw.col_patterns')">
            <template #default="scope">
              <div>
                <code v-for="pat in scope.row.patterns" :key="pat" class="block">{{ pat }}</code>
              </div>
            </template>
          </el-table-column>
        </el-table>
      </el-card>
    </div>
  </div>
  
</template>

<style scoped>
.pcgw-container { padding: 16px; }
.pcgw-result { margin-top: 16px; }
.pcgw-list { margin-top: 8px; }
.card-header { display: flex; align-items: center; }
.ml-8 { margin-left: 8px; }
.mr-4 { margin-right: 4px; }
.section-title { margin: 12px 0; font-size: 16px; }
.mt-16 { margin-top: 16px; }
.mb-12 { margin-bottom: 12px; }
.meta { margin-bottom: 12px; }
.meta-row { display: flex; gap: 8px; }
.meta-label { color: var(--el-text-color-secondary); }
.meta-value { color: var(--el-text-color-primary); }
.block { display: block; }
.options-row { display: flex; gap: 12px; align-items: center; }
.index-status-row { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
.mr-8 { margin-right: 8px; }
</style>