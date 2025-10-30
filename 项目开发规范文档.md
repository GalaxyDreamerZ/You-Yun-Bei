# Game Save Manager 项目开发规范文档

## 1. 代码风格分析

### 1.1 命名规范

#### 前端（Vue/TypeScript）
- **变量命名**：使用小驼峰命名法（camelCase），如 `showDeviceSetupDialog`、`currentDevice`
- **组件命名**：使用大驼峰命名法（PascalCase），如 `DeviceSetupDialog`、`FavoriteSideBar`
- **文件命名**：
  - Vue组件文件使用大驼峰命名法，如 `MainSideBar.vue`
  - 工具类和配置文件使用小驼峰命名法，如 `useConfig.ts`

#### 后端（Rust）
- **变量/函数命名**：使用蛇形命名法（snake_case），符合Rust语言规范
- **结构体/枚举命名**：使用大驼峰命名法（PascalCase）
- **常量命名**：使用全大写下划线分隔（SCREAMING_SNAKE_CASE）
- **模块命名**：使用蛇形命名法，如 `cloud_sync`、`path_resolver`

### 1.2 代码缩进和格式化偏好
- **前端**：使用2空格缩进
- **后端**：使用4空格缩进（Rust标准）
- **行尾**：使用LF（Unix风格换行符）
- **最大行长**：前端约120字符，后端约100字符

### 1.3 注释风格和文档标准

#### 前端
- 使用单行注释（`//`）标注代码块功能
- 重要函数前添加简短说明
- 复杂逻辑添加内联注释
- i18n标记使用 `$t('key')` 格式

#### 后端
- 使用Rust文档注释（`///`）为公共API提供文档
- 使用普通注释（`//`）解释复杂实现
- 模块级别添加模块功能说明
- 错误类型和关键结构体有详细文档

### 1.4 模块化组织方式

#### 前端
- 按功能划分目录：`components/`、`composables/`、`pages/`等
- 共享逻辑抽取为可复用的组合式函数（Composables）
- 页面组件放在`pages/`目录，遵循Nuxt约定式路由
- 全局状态和配置集中管理

#### 后端
- 按功能模块化：`backup/`、`cloud_sync/`、`config/`等
- 公共工具和类型定义放在`preclude/`目录
- 使用Rust模块系统组织代码
- 接口定义与实现分离

## 2. 架构模式总结

### 2.1 项目整体架构设计
- **Tauri框架**：使用Rust后端 + Vue前端的桌面应用架构
- **前后端通信**：通过Tauri的IPC机制（commands和events）
- **前端框架**：Nuxt 3 + Vue 3 + Element Plus
- **后端框架**：Rust + Tauri API

### 2.2 主要设计模式应用
- **组合式API**：前端大量使用Vue 3的Composition API
- **依赖注入**：通过composables提供全局服务
- **发布-订阅模式**：使用Tauri events进行前后端通信
- **命令模式**：通过Tauri commands封装后端功能
- **工厂模式**：创建不同类型的存档管理器和云同步服务

### 2.3 状态管理方案
- **前端**：
  - 使用Vue的响应式系统和组合式API管理状态
  - 配置状态通过`useConfig`组合式函数集中管理
  - 全局加载状态通过`useGlobalLoading`管理
  - 通知系统通过`useNotification`管理
- **后端**：
  - 使用Rust的所有权系统和不可变性原则管理状态
  - 配置数据集中存储和管理

### 2.4 数据流处理方式
- **前端到后端**：通过Tauri commands发送请求
- **后端到前端**：通过Tauri events发送通知和数据更新
- **异步操作**：使用Promise（前端）和async/await（两端）处理
- **错误处理**：使用Result模式（后端）和try-catch（前端）

## 3. 开发习惯提炼

### 3.1 常用工具库和依赖项

#### 前端
- **UI框架**：Element Plus
- **状态管理**：Vue 3 Composition API
- **国际化**：vue-i18n
- **工具库**：@vueuse/core、dayjs、lodash-unified、uuid
- **拖拽功能**：vuedraggable

#### 后端
- **序列化**：serde、serde_json
- **错误处理**：anyhow、thiserror
- **异步运行时**：tokio
- **文件操作**：fs_extra、zip
- **云存储**：opendal
- **日志**：log、tauri-plugin-log
- **国际化**：rust-i18n
- **音频处理**：rodio

### 3.2 测试策略和覆盖率
- 使用GitHub Actions进行CI/CD
- Rust代码使用clippy进行静态分析
- 前端主要依赖手动测试和用户反馈
- 发布前使用预发布版本进行测试

### 3.3 错误处理机制

#### 前端
- 使用try-catch捕获异常
- 通过useNotification显示友好错误信息
- 错误信息国际化处理
- 全局错误处理中间件

#### 后端
- 使用Result和Option类型处理错误
- anyhow用于内部错误传播
- thiserror定义自定义错误类型
- 详细日志记录错误信息

### 3.4 性能优化实践
- 异步处理耗时操作
- 懒加载和按需加载组件
- 使用Rust处理性能敏感操作
- 文件操作优化（流式处理、缓冲区）
- 减少不必要的UI更新

## 4. Trae适用的项目规则

### 4.1 保持与原项目一致的代码风格
- 严格遵循上述命名规范和格式化规则
- 使用ESLint和Rustfmt保持代码风格一致性
- 新代码应与周围代码保持一致的风格
- 提交前进行代码格式化检查

### 4.2 延续原有架构设计理念
- 保持前后端分离的架构设计
- 继续使用组合式API管理前端状态
- 遵循Rust的错误处理最佳实践
- 保持模块化和关注点分离

### 4.3 新增功能的开发规范
- 新功能应先创建详细设计文档
- 前端新组件应遵循现有组件结构
- 后端新模块应遵循现有模块组织方式
- 所有用户界面文本必须支持国际化
- 新功能应包含适当的错误处理和日志记录

### 4.4 代码审查标准
- 代码应符合项目风格指南
- 无重复代码，遵循DRY原则
- 适当的注释和文档
- 错误处理完善
- 性能考虑（特别是文件操作和UI渲染）
- 国际化支持完整

### 4.5 版本控制策略
- 遵循语义化版本控制（Semantic Versioning）
- 使用feature分支开发新功能
- 提交信息应清晰描述变更内容
- 重要bug修复应包含测试用例
- 发布前进行预发布测试

## 5. 具体示例和标准

### 5.1 前端组件示例
```vue
<script setup lang="ts">
// 导入依赖
import { ref, onMounted } from 'vue';
import { useConfig } from '../composables/useConfig';
import { useNotification } from '../composables/useNotification';
import { $t } from '../i18n';
import { commands } from '../bindings';

// 组合式API使用
const { config, saveConfig } = useConfig();
const { showSuccess, showError } = useNotification();

// 响应式状态
const isLoading = ref(false);
const formData = ref({
  name: '',
  path: ''
});

// 方法定义
async function handleSubmit() {
  try {
    isLoading.value = true;
    
    // 调用后端命令
    const result = await commands.saveGameData({
      name: formData.value.name,
      path: formData.value.path
    });
    
    if (result.status === 'ok') {
      showSuccess({ message: $t('success.save_completed') });
    } else {
      showError({ message: $t('error.save_failed') });
    }
  } catch (e) {
    console.error('Error saving game data:', e);
    showError({ message: $t('error.unexpected_error') });
  } finally {
    isLoading.value = false;
  }
}

// 生命周期钩子
onMounted(() => {
  // 初始化逻辑
});
</script>

<template>
  <div class="component-container">
    <h2>{{ $t('title.save_game') }}</h2>
    
    <el-form :model="formData" label-position="top">
      <el-form-item :label="$t('form.name')">
        <el-input v-model="formData.name" />
      </el-form-item>
      
      <el-form-item :label="$t('form.path')">
        <el-input v-model="formData.path" />
      </el-form-item>
      
      <el-button 
        type="primary" 
        :loading="isLoading" 
        @click="handleSubmit"
      >
        {{ $t('button.save') }}
      </el-button>
    </el-form>
  </div>
</template>
```

### 5.2 后端模块示例
```rust
//! 游戏存档管理模块
//! 提供游戏存档的备份、恢复和管理功能

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::preclude::*;

/// 游戏存档信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveInfo {
    /// 存档名称
    pub name: String,
    /// 存档路径
    pub path: PathBuf,
    /// 创建时间
    pub created_at: i64,
}

/// 游戏存档管理器
pub struct SaveManager {
    /// 配置信息
    config: Config,
}

impl SaveManager {
    /// 创建新的存档管理器
    pub fn new(config: Config) -> Self {
        Self { config }
    }
    
    /// 备份游戏存档
    /// 
    /// # 参数
    /// * `name` - 存档名称
    /// * `path` - 存档路径
    /// 
    /// # 返回
    /// 成功返回存档信息，失败返回错误
    pub async fn backup_save(&self, name: String, path: PathBuf) -> Result<SaveInfo> {
        // 检查路径是否存在
        if !path.exists() {
            return Err(Error::PathNotFound(path).into());
        }
        
        // 创建存档信息
        let save_info = SaveInfo {
            name,
            path: path.clone(),
            created_at: chrono::Utc::now().timestamp(),
        };
        
        // 执行备份操作
        self.perform_backup(&save_info)
            .context("Failed to backup save")?;
            
        Ok(save_info)
    }
    
    // 私有方法实现备份逻辑
    fn perform_backup(&self, save_info: &SaveInfo) -> Result<()> {
        // 备份实现逻辑
        // ...
        
        Ok(())
    }
}

/// 注册Tauri命令
#[tauri::command]
pub async fn backup_game_save(name: String, path: String) -> CommandResult<SaveInfo> {
    let config = get_config()?;
    let manager = SaveManager::new(config);
    
    let path = PathBuf::from(path);
    let save_info = manager.backup_save(name, path).await?;
    
    Ok(save_info)
}
```

### 5.3 国际化示例
```json
// locales/zh_SIMPLIFIED.json
{
  "title": {
    "save_game": "保存游戏",
    "load_game": "加载游戏"
  },
  "form": {
    "name": "名称",
    "path": "路径"
  },
  "button": {
    "save": "保存",
    "cancel": "取消"
  },
  "success": {
    "save_completed": "保存成功"
  },
  "error": {
    "save_failed": "保存失败",
    "unexpected_error": "发生意外错误"
  }
}
```

### 5.4 提交信息规范
```
feat(backup): 添加自动备份功能

- 实现定时自动备份功能
- 添加备份配置界面
- 优化备份性能

## 6. 总结

本规范文档详细描述了Game Save Manager项目的代码风格、架构设计、开发习惯和项目规则。在使用Trae进行二次开发时，应严格遵循这些规范，确保代码与原项目保持高度一致性和连续性。通过遵循这些规范，可以保证项目的可维护性、可扩展性和代码质量。