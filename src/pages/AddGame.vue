<script lang="ts" setup>
import {
    DocumentAdd,
    Check,
    RefreshRight,
    Download,
    QuestionFilled,
} from "@element-plus/icons-vue";
import { reactive, ref, watchEffect, computed, onUnmounted } from "vue";
import { commands, events, type Game, type SaveUnit, type Device, type ScanOptions, type ScanResult, type DetectedGame, type ScanProgressEvent } from "../bindings";
import { $t } from "../i18n";
import { v4 as uuidv4 } from 'uuid';
import { error } from "@tauri-apps/plugin-log";
import PathVariableSelector from "../components/PathVariableSelector.vue";

const route = useRoute();
const router = useRouter();
const { showError, showWarning, showSuccess } = useNotification();
const { config, refreshConfig, saveConfig } = useConfig();
const buttons = [
    {
        text: $t('addgame.search_local'),
        type: "primary",
        icon: Download,
        method: search_local,
    },
    {
        text: $t('addgame.save_current_profile'),
        type: "success",
        icon: Check,
        method: save,
    },
    {
        text: $t('addgame.reset_current_profile'),
        type: "danger",
        icon: RefreshRight,
        method: reset_info,
    },
] as const;


const game_name = ref("") // 写入游戏名
let save_paths: Array<SaveUnit> = reactive(new Array<SaveUnit>()) // 选择游戏存档目录
const game_path = ref("") // 选择游戏启动程序
const game_icon_src = ref("/orange.png")
const is_editing = ref(false) // 是否正在编辑已有的游戏
const currentDevice = ref<Device | null>(null) // 当前设备信息

// 扫描相关状态
const scanning = ref(false)
const scanProgressText = ref("")
const scanStep = ref<string>("")
const scanCurrent = ref<number>(0)
const scanTotal = ref<number>(0)
const scanOptions = reactive<ScanOptions>({
    platform: "windows",
    search_steam: true,
    search_epic: true,
    search_origin: false,
    search_registry: true,
    search_common_dirs: true,
    search_processes: false,
})
const scanResult = ref<ScanResult | null>(null)
const searchText = ref("")
let unlistenFn: (() => void) | null = null

// 存档状态缓存：避免重复计算
type SaveStatus = { hasSave: boolean; path?: string; loading: boolean }
const saveStatusMap = reactive<Record<string, SaveStatus>>({})

/**
 * 为一行生成唯一键
 * 函数级注释：基于游戏名与安装路径生成状态缓存的键，避免冲突。
 */
function statusKey(row: DetectedGame): string {
    return `${row.info?.name || ''}|${row.install_path || ''}`
}

/**
 * 计算指定游戏的存档状态
 * 函数级注释：调用后端 generateSaveUnitsForGame，仅当存在有效 SaveUnit 时判定为“有存档”，并提取当前设备路径用于展示。
 */
async function computeSaveStatus(row: DetectedGame) {
    const key = statusKey(row)
    if (!saveStatusMap[key]) {
        saveStatusMap[key] = { hasSave: false, loading: true }
    } else {
        saveStatusMap[key].loading = true
    }
    try {
        if (!row.install_path) {
            saveStatusMap[key] = { hasSave: false, loading: false }
            return
        }
        const res = await commands.generateSaveUnitsForGame(row.info, row.install_path)
        if (res.status === 'ok' && Array.isArray(res.data) && res.data.length > 0) {
            // 提取当前设备下的最佳路径（若无则取任意一个）
            let bestPath = ''
            const did = currentDevice.value?.id
            for (const u of res.data) {
                if (did && u.paths && u.paths[did]) { bestPath = u.paths[did] as string; break }
            }
            if (!bestPath) {
                for (const u of res.data) {
                    const vals = u.paths ? Object.values(u.paths) : []
                    if (vals.length) { bestPath = vals[0] as string; break }
                }
            }
            saveStatusMap[key] = { hasSave: true, path: bestPath || undefined, loading: false }
        } else {
            saveStatusMap[key] = { hasSave: false, loading: false }
        }
    } catch (e) {
        error(`Compute save status failed: ${e}`)
        saveStatusMap[key] = { hasSave: false, loading: false }
    }
}

/**
 * 获取指定行的存档状态
 * 函数级注释：从缓存映射中读取状态，若不存在则返回 undefined。
 */
function getSaveStatus(row: DetectedGame): SaveStatus | undefined {
    return saveStatusMap[statusKey(row)]
}

// 编辑对话框相关状态
const editDialogVisible = ref(false)
const editRowIndex = ref<number | null>(null)
const editOriginalPath = ref("")
const resolvedPreview = ref("")

/**
 * 打开编辑对话框
 * 函数级注释：记录当前行索引与原始路径，弹出模态框并生成解析预览。
 */
async function openEditDialog(index: number) {
    editRowIndex.value = index
    if (currentDevice.value) {
        const row = save_paths[index]
        const path = (row.paths && currentDevice.value) ? (row.paths[currentDevice.value.id] || "") : ""
        editOriginalPath.value = path
        editDialogVisible.value = true
        await updatePreview()
    }
}

/**
 * 更新解析预览
 * 函数级注释：调用后端 resolvePath 将变量模板解析为真实路径并展示。
 */
async function updatePreview() {
    try {
        const res = await commands.resolvePath(editOriginalPath.value)
        if (res.status === 'ok') {
            resolvedPreview.value = res.data
        } else {
            resolvedPreview.value = ''
        }
    } catch {
        resolvedPreview.value = ''
    }
}

/**
 * 应用编辑结果
 * 函数级注释：将编辑路径写回当前设备对应的 SaveUnit 路径，并关闭对话框。
 */
function applyEdit() {
    if (editRowIndex.value == null || !currentDevice.value) return
    const row = save_paths[editRowIndex.value]
    if (!row.paths) row.paths = {}
    row.paths[currentDevice.value.id] = editOriginalPath.value
    editDialogVisible.value = false
}

// 获取当前设备信息
async function fetchCurrentDevice() {
    try {
        const result = await commands.getCurrentDeviceInfo();
        if (result.status === "ok") {
            currentDevice.value = result.data;
            console.log("Current device:", currentDevice.value);
        } else {
            showError({ message: result.error });
        }
    } catch (e) {
        error(`Error getting current device info: ${e}`);
        showError({ message: $t('error.get_device_info_failed') });
    }
}
// 在组件挂载时获取当前设备信息
fetchCurrentDevice();

/**
 * 订阅扫描进度事件
 * 函数级注释：在开始扫描前调用，记录每个阶段的进度并更新 UI。
 */
async function subscribeScanProgress() {
    try {
        unlistenFn = await events.scanProgress.listen((ev) => {
            const payload = ev.payload as ScanProgressEvent
            scanStep.value = payload.step
            scanCurrent.value = payload.current
            scanTotal.value = payload.total
            scanProgressText.value = payload.message || ""
        })
    } catch (e) {
        error(`Subscribe scan progress failed: ${e}`)
    }
}

onUnmounted(() => {
    if (unlistenFn) {
        unlistenFn()
        unlistenFn = null
    }
})

/**
 * 启动自动扫描
 * 函数级注释：调用后端 scan_games(options) 并接收结果，更新表格数据。
 */
async function startScan() {
    if (scanning.value) return
    try {
        await subscribeScanProgress()
        scanning.value = true
        scanResult.value = null
        const res = await commands.scanGames(scanOptions)
        if (res.status === "ok") {
            scanResult.value = res.data
        } else {
            showError({ message: res.error })
        }
    } catch (e) {
        error(`Scan error: ${e}`)
        showError({ message: $t('error.unexpected_error') })
    } finally {
        scanning.value = false
        if (unlistenFn) { unlistenFn(); unlistenFn = null }
    }
}

/**
 * 过滤检测到的游戏用于显示列表
 * 函数级注释：根据搜索文本对名称进行包含过滤。
 */
const filteredDetected = computed(() => {
    if (!scanResult.value) return []
    const q = searchText.value.trim().toLowerCase()
    return scanResult.value.detected.filter((d) => {
        const name = (d.info?.name || '').toLowerCase()
        return q === '' || name.includes(q)
    })
})

/**
 * 统计全部检测到的游戏数量
 * 函数级注释：基于 scanResult 的 detected 列表计算总数。
 */
const totalDetectedCount = computed(() => {
    return scanResult.value?.detected.length || 0
})

/**
 * 统计有存档的游戏数量
 * 函数级注释：遍历所有检测到的游戏，统计已计算为 hasSave 的数量。
 */
const detectedWithSaveCount = computed(() => {
    if (!scanResult.value) return 0
    let c = 0
    for (const row of scanResult.value.detected) {
        const st = saveStatusMap[statusKey(row)]
        if (st && st.hasSave) c++
    }
    return c
})

// 当列表变化时，异步预计算每行的存档状态
watchEffect(() => {
    for (const row of filteredDetected.value) {
        const key = statusKey(row)
        if (!saveStatusMap[key]) {
            // 异步触发计算
            computeSaveStatus(row)
        }
    }
})

/**
 * 预计算全部检测到的游戏的存档状态
 * 函数级注释：确保顶部统计中的“有存档数量”准确。
 */
watchEffect(() => {
    if (!scanResult.value) return
    for (const row of scanResult.value.detected) {
        const key = statusKey(row)
        if (!saveStatusMap[key]) {
            computeSaveStatus(row)
        }
    }
})

// init info when navigate from GameManage.vue
watchEffect(() => {
    const gameName = route.params.name;
    if (gameName) {
        const gameConfig = config.value?.games.find(game => game.name === gameName);
        if (gameConfig) {
            is_editing.value = true;
            game_name.value = gameConfig.name;
            save_paths = gameConfig.save_paths;
            
            // 获取当前设备的游戏路径
            if (gameConfig.game_paths && currentDevice.value) {
                const deviceId = currentDevice.value.id;
                game_path.value = gameConfig.game_paths[deviceId] || '';
            } else {
                game_path.value = '';
            }
        } else {
            showError({ message: $t('addgame.change_target_not_exists_error') + gameName });
            router.back();
        }
    }
});

function check_save_unit_unique(p: string) {
    // 检查是否有任何存档单元的任何设备路径与新路径相同
    if (save_paths.find((x) => {
        if (!x.paths) return false;
        return Object.values(x.paths).includes(p);
    })) {
        showWarning({ message: $t('addgame.duplicated_filename_error') });
        return false;
    }
    return true;
}
function check_name_valid(name: string) {
    let invalid_reg = RegExp(/[<>:"\/\\|?*]/);
    return !invalid_reg.test(name);
}
function generate_save_unit(unit_type: "Folder" | "File", path: string): SaveUnit {
    let delete_before_apply = config.value?.settings.default_delete_before_apply;
    
    // 创建一个基本的 SaveUnit，使用当前设备ID作为路径映射的键
    const saveUnit: SaveUnit = {
        unit_type,
        paths: {},
        delete_before_apply
    };
    
    // 如果有当前设备信息，则添加路径
    if (currentDevice.value) {
        const deviceId = currentDevice.value.id;
        saveUnit.paths![deviceId] = path;
    }
    
    return saveUnit;
}

// 在路径输入框中插入变量
function insertPathVariable(variable: string, inputRef: any) {
    if (!inputRef) return;
    
    const input = inputRef.$el.querySelector('input');
    if (!input) return;
    
    const start = input.selectionStart || 0;
    const end = input.selectionEnd || 0;
    const value = input.value;
    
    // 在光标位置插入变量
    const newValue = value.substring(0, start) + variable + value.substring(end);
    input.value = newValue;
    
    // 触发输入事件，确保值被更新
    input.dispatchEvent(new Event('input', { bubbles: true }));
    
    // 设置光标位置到插入的变量之后
    const newPosition = start + variable.length;
    setTimeout(() => {
        input.setSelectionRange(newPosition, newPosition);
        input.focus();
    }, 0);
}

async function add_save_directory() {
    try {
        const dir = await commands.chooseSaveDir();
        if (dir.status == "error" || !check_save_unit_unique(dir.data)) { return; }
        save_paths.push(
            generate_save_unit("Folder", dir.data)
        );
    } catch (e) {
        error(`Error choosing save directory: ${e}`)
        showError({ message: $t('error.choose_save_dir_error') });
    }
}

async function add_save_file() {
    try {
        const file = await commands.chooseSaveFile();
        if (file.status == "error" || !check_save_unit_unique(file.data)) { return; }
        save_paths.push(
            generate_save_unit("File", file.data)
        );
    } catch (e) {
        error(`Error choosing save file: ${e}`)
        showError({ message: $t('error.choose_save_file_error') });
    }
}

async function choose_executable_file() {
    try {
        const file = await commands.chooseSaveFile();
        if (file.status == "error") { return; }
        game_path.value = file.data;
    } catch (e) {
        error(`Error choosing executable file: ${e}`)
        showError({ message: $t('error.choose_executable_file_error') });
    }
}

function submit_handler(button_method: Function) {
    // 映射按钮的ID和他们要触发的方法
    button_method();
}
function search_local() {
    // 启动扫描面板动作
    startScan()
}
async function save() {
    // 去除头尾空字符，防止触发Windows文件命名规则问题
    game_name.value = game_name.value.trim();
    if (game_name.value == "" || save_paths.length == 0) {
        showError({ message: $t('addgame.no_name_error') });
        return;
    }
    if (!check_name_valid(game_name.value)) {
        showError({ message: $t('addgame.invalid_name_error') });
        return;
    }
    if (config.value?.games.find((x) => x.name.toLowerCase() == game_name.value.toLowerCase())) {
        showError({ message: $t('addgame.duplicated_name_error') });
        return;
    }
    let game: Game = {
        name: game_name.value,
        save_paths: save_paths,
    };

    // 如果有游戏路径和当前设备信息，则添加游戏路径
    if (game_path.value && currentDevice.value) {
        game.game_paths = {};
        game.game_paths[currentDevice.value.id] = game_path.value;
    }
    try {
        const result = await commands.addGame(game);

        if (is_editing.value) {
            is_editing.value = false;
            showSuccess({ message: $t('addgame.add_game_success') });
            router.back();
        } else {
            if (config.value?.settings.add_new_to_favorites) {
                // TODO:以下内容是否需要抽离成单独的工具库？还是说应该后端处理？
                await refreshConfig();
                config.value?.favorites?.push({
                    label: game.name,
                    is_leaf: true,
                    children: [],
                    node_id: uuidv4().toString()
                });
                await saveConfig();
            }
            showSuccess({ message: $t('addgame.add_game_success') });
        }
        reset_info(false);
        await refreshConfig();
    } catch (e) {
        error(`Error adding game: ${e}`);
        showError({ message: $t('error.add_game_failed') });
    }
}
function reset_info(show_notification: boolean = true) {
    // 重置当前配置
    game_name.value = "";
    save_paths = reactive([]);
    game_path.value = "";
    // TODO:This is a first occurrence of a i18n text duplication. How to handle this?
    if (show_notification) { showSuccess({ message: $t('settings.reset_success') }); }
}

function deleteRow(index: number) {
    save_paths.splice(index, 1);
}


/**
 * 一键添加：将检测到的游戏直接写入配置
 * 函数级注释：生成 SaveUnit 并调用 addGame，随后刷新配置。
 */
async function quickAdd(row: DetectedGame) {
    try {
        const unitsRes = row.install_path
            ? await commands.generateSaveUnitsForGame(row.info, row.install_path)
            : { status: 'ok', data: [] } as any
        if (unitsRes.status === 'error') {
            showError({ message: unitsRes.error })
            return
        }
        const game: Game = { name: row.info.name, save_paths: unitsRes.data }
        if (row.install_path && currentDevice.value) {
            game.game_paths = {}
            game.game_paths[currentDevice.value.id] = row.install_path
        }
        const result = await commands.addGame(game)
        if (result.status === 'ok') {
            showSuccess({ message: $t('addgame.add_game_success') })
            await refreshConfig()
        } else {
            showError({ message: result.error })
        }
    } catch (e) {
        error(`Quick add failed: ${e}`)
        showError({ message: $t('error.add_game_failed') })
    }
}
</script>

<template>
    <div class="select-container">
        <el-card class="game-info">
            <div class="top-part">
                <img class="game-icon" :src="game_icon_src" />
                <div class="bold">
                    {{ $t("addgame.warning_for_save_file") }}
                </div>
                <el-input v-model="game_name" :placeholder="$t('addgame.input_game_name_prompt')" class="game-name">
                    <template #prepend>
                        {{ $t('addgame.game_name') }} </template>
                </el-input>
                <el-input v-model="game_path" :placeholder="$t('addgame.input_game_launch_path_prompt')"
                    class="game-path">
                    <template #prepend>
                        {{ $t('addgame.game_launch_path') }} </template>
                    <template #append>
                        <el-button @click="choose_executable_file()">
                            <el-icon>
                                <document-add />
                            </el-icon>
                        </el-button>
                    </template>
                </el-input>
            </div>
            <div class="add-button-area">
                <div class="button-row">
                    <el-button type="primary" @click="add_save_directory">{{ $t('addgame.add_save_directory') }}</el-button>
                    <el-button type="primary" @click="add_save_file">{{ $t('addgame.add_save_file') }}</el-button>
                </div>
                <div class="path-variable-info">
                    <el-alert
                        type="info"
                        :closable="false"
                        show-icon
                    >
                        {{ $t('addgame.path_variable_hint') }}
                    </el-alert>
                </div>
            </div>
            <el-table :data="save_paths" class="save-table">
                <el-table-column fixed prop="unit_type" :label="$t('addgame.type')" width="120" />
                <el-table-column :label="$t('addgame.operations')" width="120">
                    <template #default="scope">
                        <el-button link type="primary" size="small" @click.prevent="deleteRow(scope.$index)">
                            {{ $t('addgame.remove') }} </el-button>
                        <el-button link type="primary" size="small" @click.prevent="openEditDialog(scope.$index)">
                            {{ $t('addgame.edit') }}
                        </el-button>
                    </template>
                </el-table-column>
                <el-table-column :label="$t('addgame.path')" min-width="300">
                    <template #default="scope">
                        <div class="path-input-container">
                            <el-input
                                :model-value="scope.row.paths && currentDevice ? scope.row.paths[currentDevice.id] || '' : ''"
                                size="small"
                                @update:model-value="(value) => {
                                    if (currentDevice && scope.row.paths) {
                                        scope.row.paths[currentDevice.id] = value;
                                    }
                                }"
                            >
                                <template #append>
                                    <path-variable-selector
                                        :current-path="scope.row.paths && currentDevice ? scope.row.paths[currentDevice.id] || '' : ''"
                                        @insert="(variable) => {
                                            if (currentDevice && scope.row.paths) {
                                                const currentPath = scope.row.paths[currentDevice.id] || '';
                                                scope.row.paths[currentDevice.id] = currentPath + variable;
                                            }
                                        }"
                                    />
                                </template>
                            </el-input>
                        </div>
                    </template>
                </el-table-column>
                <el-table-column :label="$t('addgame.device_info')" width="200" v-if="currentDevice">
                    <template #default>
                        <el-tag size="small">{{ currentDevice?.name }}</el-tag>
                    </template>
                </el-table-column>
            </el-table>
        </el-card>
        <!-- 自动扫描面板（位置下移至手动添加之后，仅调整UI顺序） -->
        <el-card class="game-info">
            <div style="display:flex;align-items:center;justify-content:space-between;gap:12px;">
                <div>{{ $t('scan.panel_title') }}</div>
                <el-button type="primary" :loading="scanning" @click="startScan">{{ $t('scan.start_scan') }}</el-button>
            </div>
            <div style="margin-top:12px;display:flex;flex-wrap:wrap;gap:12px;">
                <el-checkbox v-model="scanOptions.search_steam">Steam</el-checkbox>
                <el-checkbox v-model="scanOptions.search_epic">Epic</el-checkbox>
                <el-checkbox v-model="scanOptions.search_origin">Origin/EA</el-checkbox>
                <el-checkbox v-model="scanOptions.search_registry">{{ $t('scan.registry') }}</el-checkbox>
                <el-checkbox v-model="scanOptions.search_common_dirs">{{ $t('scan.common_dirs') }}</el-checkbox>
                <el-checkbox v-model="scanOptions.search_processes">{{ $t('scan.processes') }}</el-checkbox>
            </div>
            <div style="margin-top:12px;">
                <el-progress :percentage="scanTotal ? Math.round((scanCurrent/scanTotal)*100) : 0" />
                <div style="margin-top:6px;color:#888;">{{ scanStep }} {{ scanProgressText }}</div>
            </div>
            <div style="margin-top:12px;display:flex;align-items:center;gap:8px;">
                <el-input v-model="searchText" :placeholder="$t('misc.search')" style="max-width:280px;" />
                <span style="color:#888;">检测到{{ totalDetectedCount }}个游戏，其中{{ detectedWithSaveCount }}个游戏有存档</span>
            </div>
            <el-table :data="filteredDetected" style="width:100%;margin-top:8px;" size="small" border>
                <el-table-column :label="$t('addgame.game_name')" prop="info.name" min-width="180" />
                <el-table-column :label="$t('scan.col.source')" prop="source" min-width="120" />
                <el-table-column :label="$t('scan.col.install_path')" prop="install_path" min-width="260" />
                <el-table-column :label="$t('scan.col.save_status')" min-width="280">
                    <template #default="{ row }">
                        <div v-if="getSaveStatus(row)?.loading" style="color:#888;">...</div>
                        <div v-else>
                            <el-tag :type="getSaveStatus(row)?.hasSave ? 'success' : 'info'" size="small">
                                {{ getSaveStatus(row)?.hasSave ? $t('scan.has_save') : $t('scan.no_save') }}
                            </el-tag>
                            <div v-if="getSaveStatus(row)?.hasSave" style="margin-top:4px;word-break:break-all;">
                                {{ getSaveStatus(row)?.path || $t('scan.unknown_path') }}
                            </div>
                        </div>
                    </template>
                </el-table-column>
                <el-table-column :label="$t('addgame.operations')" min-width="160">
                    <template #default="{ row }">
                        <el-button type="success" size="small" :disabled="!(getSaveStatus(row)?.hasSave)" @click="quickAdd(row)">{{ $t('scan.quick_add') }}</el-button>
                    </template>
                </el-table-column>
            </el-table>
        </el-card>
        <el-dialog v-model="editDialogVisible" :title="$t('addgame.edit')" width="500px">
            <div style="display:flex;flex-direction:column;gap:12px;">
                <el-input v-model="editOriginalPath" @input="updatePreview" />
                <el-alert type="info" :closable="false" show-icon>
                    {{ $t('addgame.preview_resolved') }}: {{ resolvedPreview || $t('scan.unknown_path') }}
                </el-alert>
            </div>
            <template #footer>
                <el-button @click="editDialogVisible=false">{{ $t('settings.cancel') }}</el-button>
                <el-button type="primary" @click="applyEdit">{{ $t('settings.confirm') }}</el-button>
            </template>
        </el-dialog>
        <el-container class="submit-bar">
            <el-tooltip v-for="button in buttons" :key="button.text" :content="button.text" placement="top">
                <el-button @click="submit_handler(button.method)" :type="button.type" circle>
                    <el-icon>
                        <component :is="button.icon" />
                    </el-icon>
                </el-button>
            </el-tooltip>
        </el-container>
    </div>
</template>

<style scoped>
.bold {
    margin-left: 10px;
    font-weight: bold;
    color: var(--el-text-color-primary);
}

.save-table {
    margin-top: 20px;
    margin-bottom: 20px;
}

.select-container {
    height: 90%;
    width: 100%;
}

.el-card {
    margin-bottom: 15px;
    padding-bottom: 20px;
}

.top-part {
    height: 200px;
    display: grid;
    grid-template-columns: 1fr 3fr;
    grid-template-rows: 1fr 1fr 1fr 1fr 1fr 1fr;
}

.top-part>img {
    grid-column: 1/2;
    grid-row: 1/7;
    margin: auto;
}

.game-name {
    grid-column: 2/3;
    grid-row: 5/6;
    margin-bottom: 5px;
}

.game-path {
    grid-column: 2/3;
    grid-row: 6/7;
}

.game-icon {
    float: left;
    height: 200px;
    width: 200px;
}

.add-button-area {
    margin-top: 20px;
}

.button-row {
    display: flex;
    gap: 10px;
    margin-bottom: 10px;
}

.path-variable-info {
    margin-top: 10px;
    margin-bottom: 10px;
}

.path-input-container {
    display: flex;
    align-items: center;
}

.submit-bar {
    justify-content: flex-end;
    height: 10%;
}
</style>