<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { ElButton, ElCard, ElDivider, ElTag, ElTable, ElTableColumn, ElInput, ElSelect, ElOption } from 'element-plus'
import { $t } from '~/i18n'
import { commands, events, type ScanProgressEvent, type ScanOptions, type ScanResult } from '~/bindings'
import { useNotification } from '~/composables/useNotification'

// 响应式状态
const isScanning = ref(false)
const progress = ref<Array<ScanProgressEvent>>([])
const result = ref<ScanResult | null>(null)
const isTauriEnv = ref(false)
let unlistenFn: (() => void) | null = null

// 扫描选项（可根据需要调整，当前为 Windows 常用组合）
const options = ref<ScanOptions>({
  platform: 'windows',
  search_steam: true,
  search_epic: false,
  search_origin: true,
  search_registry: true,
  search_common_dirs: true,
  search_processes: false,
})

const { showError, showSuccess, showInfo } = useNotification()

// 过滤与搜索状态
const searchText = ref('')
const selectedSources = ref<string[]>([])

/**
 * 计算当前结果中可用的来源列表
 *
 * - 当没有结果时返回空数组
 * - 来源取自 detected 项的 `source`
 */
const availableSources = computed<string[]>(() => {
  if (!result.value) return []
  const set = new Set<string>()
  for (const it of result.value.detected) set.add(it.source)
  return Array.from(set)
})

/**
 * 过滤“检测到的游戏”表格数据
 *
 * - 按来源 `selectedSources` 过滤；为空表示不过滤
 * - 按名称 `searchText` 进行包含匹配（大小写不敏感）
 */
const filteredDetected = computed(() => {
  if (!result.value) return []
  const bySource = (row: any) =>
    selectedSources.value.length === 0 || selectedSources.value.includes(row.source)
  const byName = (row: any) => {
    const name = (row.info?.name || '').toLowerCase()
    const q = searchText.value.trim().toLowerCase()
    return q === '' || name.includes(q)
  }
  return result.value.detected.filter((r) => bySource(r) && byName(r))
})

/**
 * 过滤“匹配到的存档”表格数据
 *
 * - 使用 `searchText` 对 `rule_id` 和 `resolved_path` 做包含匹配
 */
const filteredMatches = computed(() => {
  if (!result.value) return []
  const q = searchText.value.trim().toLowerCase()
  if (q === '') return result.value.matches
  return result.value.matches.filter((m: any) => {
    const rid = String(m.rule_id || '').toLowerCase()
    const path = String(m.resolved_path || '').toLowerCase()
    return rid.includes(q) || path.includes(q)
  })
})

/**
 * 检测是否运行在 Tauri 环境
 *
 * - 逻辑：通过 window.__TAURI__.core.invoke 判断
 * - 作用：浏览器模式下禁用命令调用与事件订阅
 */
function detectTauriEnv(): void {
  const w: any = typeof window !== 'undefined' ? window : undefined
  // 兼容 Tauri v1/v2：同时检测 core.invoke 与 tauri.invoke；避免在桌面模式下误判为浏览器
  const hasTauri = !!(w && w.__TAURI__)
  const hasInvokeV2 = !!(w && w.__TAURI__ && w.__TAURI__.core && typeof w.__TAURI__.core.invoke === 'function')
  const hasInvokeV1 = !!(w && w.__TAURI__ && w.__TAURI__.tauri && typeof w.__TAURI__.tauri.invoke === 'function')
  // 额外兜底：某些环境下 UA 包含 Tauri 标识
  const hasUAFlag = typeof navigator !== 'undefined' && /tauri/i.test(navigator.userAgent)
  isTauriEnv.value = hasTauri && (hasInvokeV2 || hasInvokeV1 || hasUAFlag)
}

/**
 * 通过调用后端命令进行一次“探测”，以更可靠地判断是否处于 Tauri 环境
 *
 * - 行为：调用 `getCurrentDeviceInfo`，若成功则判定为 Tauri 环境
 * - 目的：规避由于注入时序导致的全局对象检测误判
 */
async function probeTauriEnv(): Promise<void> {
  try {
    const res = await commands.getCurrentDeviceInfo()
    if (res && res.status === 'ok') {
      isTauriEnv.value = true
    }
  } catch {
    // 忽略错误，保持现有检测结果
  }
}

/**
 * 订阅扫描进度事件
 *
 * - 事件源：`events.scanProgress`（tauri-specta 生成）
 * - 清理：组件卸载时调用 `unlistenFn()` 取消订阅
 */
async function subscribeProgress(): Promise<void> {
  if (!isTauriEnv.value) return
  try {
    const unlisten = await events.scanProgress.listen((ev) => {
      // 使用步骤去重：同一步骤只保留最新一条，避免刷屏
      const payload = ev.payload as ScanProgressEvent
      updateProgress(payload)
    })
    unlistenFn = unlisten
  } catch (e) {
    showError({ message: $t('scan.subscribe_failed') })
  }
}

/**
 * 取消订阅扫描进度事件
 */
function unsubscribeProgress(): void {
  try {
    if (unlistenFn) {
      unlistenFn()
      unlistenFn = null
    }
  } catch {
    // 忽略取消失败
  }
}

/**
 * 根据步骤代码映射友好的中文文案
 *
 * - 输入：如 "index_load"、"detect_games"、"match_saves" 等
 * - 输出：优先使用 i18n 中的映射，缺失时回退原始代码
 */
function getStepLabel(step: string): string {
  const key = `scan.steps.${step}`
  const translated = $t(key)
  // $t 返回 key 本身表示未配置，做回退
  if (translated === key) return step
  return translated
}

/**
 * 维护进度列表：按步骤进行去重并更新为最新消息
 *
 * - 行为：若列表中已有相同 step，替换为最新；否则追加
 */
function updateProgress(p: ScanProgressEvent): void {
  const idx = progress.value.findIndex((x) => x.step === p.step)
  if (idx >= 0) progress.value[idx] = p
  else progress.value.push(p)
}

/**
 * 触发扫描并接收结果
 *
 * - 行为：调用 `commands.scanGames(options)` 并展示返回的结果统计
 * - 错误：以通知形式提示
 */
async function startScan(): Promise<void> {
  // 点击时重试环境检测，并进行一次命令探测，避免初始化时误判
  if (!isTauriEnv.value) {
    detectTauriEnv()
    await probeTauriEnv()
  }
  if (!isTauriEnv.value) {
    showInfo({ message: $t('scan.desktop_only') })
    return
  }
  try {
    isScanning.value = true
    progress.value = []
    result.value = null
    const res = await commands.scanGames(options.value)
    if (res.status === 'ok') {
      result.value = res.data
      showSuccess({ message: $t('scan.completed') })
    } else {
      showError({ message: res.error })
    }
  } catch (e) {
    showError({ message: $t('scan.unexpected_error') })
  } finally {
    isScanning.value = false
  }
}

/**
 * 复制文本到剪贴板（Tauri/浏览器双环境兼容）
 *
 * - 优先使用 Tauri API；失败时回退到 `navigator.clipboard`
 */
async function copyText(text: string): Promise<void> {
  try {
    const w: any = typeof window !== 'undefined' ? window : undefined
    if (w && w.__TAURI__ && w.__TAURI__.clipboard && typeof w.__TAURI__.clipboard.writeText === 'function') {
      await w.__TAURI__.clipboard.writeText(text)
    } else if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
      await navigator.clipboard.writeText(text)
    } else {
      // 兜底方案：创建输入框选中复制
      const input = document.createElement('input')
      input.value = text
      document.body.appendChild(input)
      input.select()
      document.execCommand('copy')
      document.body.removeChild(input)
    }
    showSuccess({ message: $t('scan.copied_ok') })
  } catch {
    showError({ message: $t('scan.copy_failed') })
  }
}

/**
 * 导出当前过滤后的结果为 JSON 文件
 *
 * - 仅在前端生成并触发下载，不依赖后端
 */
function exportFilteredToJSON(): void {
  const detected = filteredDetected.value
  const matches = filteredMatches.value
  if (!detected.length && !matches.length) {
    showInfo({ message: $t('scan.no_data_to_export') })
    return
  }
  const payload = {
    generated_at: new Date().toISOString(),
    filters: {
      sources: selectedSources.value,
      keyword: searchText.value.trim(),
    },
    detected,
    matches,
  }
  const json = JSON.stringify(payload, null, 2)
  const blob = new Blob([json], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `scan-results-${Date.now()}.json`
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}

/**
 * 生命周期：挂载时检测环境并订阅事件；卸载时清理订阅
 */
onMounted(async () => {
  detectTauriEnv()
  await probeTauriEnv()
  await subscribeProgress()
})

onUnmounted(() => {
  unsubscribeProgress()
})
</script>

<template>
  <div class="scan-page">
    <h2>{{ $t('scan.title') }}</h2>

    <ElCard class="scan-card">
      <div class="options">
        <ElTag type="info">{{ $t('scan.platform') }}: {{ options.platform }}</ElTag>
        <ElTag :type="options.search_steam ? 'success' : 'warning'">Steam</ElTag>
        <ElTag :type="options.search_origin ? 'success' : 'warning'">Origin/EA</ElTag>
        <ElTag :type="options.search_common_dirs ? 'success' : 'warning'">{{ $t('scan.common_dirs') }}</ElTag>
      </div>

      <ElDivider />

      <div class="actions">
        <ElButton type="primary" :loading="isScanning" @click="startScan">
          {{ $t('scan.start') }}
        </ElButton>
        <ElInput
          v-model="searchText"
          class="action-input"
          size="small"
          clearable
          :placeholder="$t('scan.search_placeholder')"
        />
        <ElSelect
          v-model="selectedSources"
          class="action-select"
          size="small"
          multiple
          collapse-tags
          :placeholder="$t('scan.filter_source_placeholder')"
        >
          <ElOption
            v-for="s in availableSources"
            :key="s"
            :label="$t(`scan.source.${s}`)"
            :value="s"
          />
        </ElSelect>
        <ElButton @click="exportFilteredToJSON" size="small">
          {{ $t('scan.export_json') }}
        </ElButton>
      </div>

      <ElDivider />

      <div class="progress">
        <h3>{{ $t('scan.progress') }}</h3>
        <ul>
          <li v-for="(p, idx) in progress" :key="idx">
            <ElTag>{{ getStepLabel(p.step) }}</ElTag>
            <span class="progress-text">{{ p.current }} / {{ p.total }} - {{ p.message || '' }}</span>
          </li>
        </ul>
      </div>

      <ElDivider />

      <div class="result" v-if="result">
        <h3>{{ $t('scan.result') }}</h3>
        <p>
          {{ $t('scan.detected_count') }}: {{ result.detected.length }}
          &nbsp;|&nbsp;
          {{ $t('scan.matched_count') }}: {{ result.matches.length }}
          &nbsp;|&nbsp;
          {{ $t('scan.errors_count') }}: {{ result.errors.length }}
        </p>

        <!-- 检测到的游戏列表 -->
        <ElDivider />
        <h4>{{ $t('scan.detected_table') }}</h4>
        <ElTable :data="filteredDetected" size="small" border>
          <ElTableColumn :label="$t('scan.col.name')" min-width="180">
            <template #default="{ row }">
              {{ row.info?.name || '-' }}
            </template>
          </ElTableColumn>
          <ElTableColumn :label="$t('scan.col.source')" min-width="120">
            <template #default="{ row }">
              {{ $t(`scan.source.${row.source}`) }}
            </template>
          </ElTableColumn>
          <ElTableColumn :label="$t('scan.col.install_path')" min-width="280">
            <template #default="{ row }">
              {{ row.install_path ?? $t('scan.unknown_path') }}
            </template>
          </ElTableColumn>
          <ElTableColumn :label="$t('scan.col.actions')" min-width="160">
            <template #default="{ row }">
              <ElButton size="small" @click="copyText(row.install_path || '')">
                {{ $t('scan.copy_path') }}
              </ElButton>
            </template>
          </ElTableColumn>
        </ElTable>

        <!-- 匹配到的存档路径列表 -->
        <ElDivider />
        <h4>{{ $t('scan.matches_table') }}</h4>
        <ElTable :data="filteredMatches" size="small" border>
          <ElTableColumn :label="$t('scan.col.rule_id')" min-width="160" prop="rule_id" />
          <ElTableColumn :label="$t('scan.col.resolved_path')" min-width="320" prop="resolved_path" />
          <ElTableColumn :label="$t('scan.col.exists')" min-width="100">
            <template #default="{ row }">
              <ElTag :type="row.exists ? 'success' : 'danger'">
                {{ row.exists ? $t('scan.yes') : $t('scan.no') }}
              </ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="$t('scan.col.confidence')" min-width="120">
            <template #default="{ row }">{{ (row.confidence ?? 0).toFixed(2) }}</template>
          </ElTableColumn>
          <ElTableColumn :label="$t('scan.col.actions')" min-width="160">
            <template #default="{ row }">
              <ElButton size="small" @click="copyText(row.resolved_path || '')">
                {{ $t('scan.copy_path') }}
              </ElButton>
            </template>
          </ElTableColumn>
        </ElTable>
      </div>
    </ElCard>
  </div>
</template>

<style scoped>
.scan-page {
  padding: 16px;
}
.scan-card {
  margin-top: 8px;
}
.options {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.actions {
  display: flex;
  gap: 8px;
}
.action-input {
  width: 280px;
}
.action-select {
  width: 220px;
}
.progress ul {
  list-style: none;
  padding: 0;
}
.progress-text {
  margin-left: 8px;
}
</style>