<script setup lang="ts">
import { ref, computed } from 'vue'
import {
  NButton,
  NIcon,
  NImage,
  NTag,
  NSpace,
  useMessage,
} from 'naive-ui'
import { Image24Regular, Dismiss24Regular } from '@vicons/fluent'
import { compressImage, type ImageCompressResult } from '../libs/commands'

interface ImageAttachment {
  id: string
  data: string
  mimeType: string
  name: string
  size: number
  compressed?: ImageCompressResult
}

const emit = defineEmits<{
  (e: 'images-selected', images: ImageAttachment[]): void
  (e: 'images-cleared'): void
}>()

const notification = useMessage()
const selectedImages = ref<ImageAttachment[]>([])
const isProcessing = ref(false)

const hasImages = computed(() => selectedImages.value.length > 0)

const generateId = () => Math.random().toString(36).substring(2, 9)

const handleFileSelect = async (event: Event) => {
  const input = event.target as HTMLInputElement
  const files = input.files
  if (!files || files.length === 0) return

  isProcessing.value = true

  try {
    const newImages: ImageAttachment[] = []

    for (const file of Array.from(files)) {
      // Check file type
      if (!file.type.startsWith('image/')) {
        notification.warning(`${file.name} 不是图片文件`)
        continue
      }

      // Check file size (max 20MB)
      if (file.size > 20 * 1024 * 1024) {
        notification.warning(`${file.name} 超过 20MB 限制`)
        continue
      }

      // Read file as base64
      const base64 = await readFileAsBase64(file)
      
      // Compress image using backend
      try {
        const compressed = await compressImage(
          base64,
          file.type,
          {
            max_width: 2048,
            max_height: 2048,
            jpeg_quality: 85,
          }
        )

        newImages.push({
          id: generateId(),
          data: compressed.data,
          mimeType: compressed.mime_type,
          name: file.name,
          size: compressed.compressed_size,
          compressed,
        })

        if (compressed.was_compressed) {
          const ratio = ((1 - compressed.compressed_size / compressed.original_size) * 100).toFixed(1)
          notification.success(`${file.name} 已压缩 ${ratio}%`)
        }
      } catch (e) {
        console.error('Image compression failed:', e)
        // Use original if compression fails
        newImages.push({
          id: generateId(),
          data: base64,
          mimeType: file.type,
          name: file.name,
          size: file.size,
        })
      }
    }

    selectedImages.value.push(...newImages)
    emit('images-selected', selectedImages.value)
  } catch (e) {
    notification.error('图片处理失败: ' + e)
  } finally {
    isProcessing.value = false
    // Reset input
    const input = document.getElementById('image-input') as HTMLInputElement
    if (input) input.value = ''
  }
}

const readFileAsBase64 = (file: File): Promise<string> => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = reader.result as string
      // Remove data URL prefix
      const base64 = result.split(',')[1]
      resolve(base64)
    }
    reader.onerror = reject
    reader.readAsDataURL(file)
  })
}

const removeImage = (id: string) => {
  selectedImages.value = selectedImages.value.filter(img => img.id !== id)
  if (selectedImages.value.length === 0) {
    emit('images-cleared')
  } else {
    emit('images-selected', selectedImages.value)
  }
}

const clearImages = () => {
  selectedImages.value = []
  emit('images-cleared')
}

const handleUploadClick = () => {
  const input = document.getElementById('image-input') as HTMLInputElement
  input?.click()
}

// Expose for parent component
const getImagesForMessage = () => {
  return selectedImages.value.map(img => ({
    type: 'image_url',
    image_url: {
      url: `data:${img.mimeType};base64,${img.data}`,
    },
  }))
}

defineExpose({
  getImagesForMessage,
  clearImages,
  hasImages,
})
</script>

<template>
  <div class="image-input-container">
    <!-- Selected Images Preview -->
    <div v-if="hasImages" class="images-preview">
      <n-space :wrap="true" size="small">
        <div
          v-for="image in selectedImages"
          :key="image.id"
          class="image-preview-item"
        >
          <n-image
            :src="`data:${image.mimeType};base64,${image.data}`"
            :alt="image.name"
            class="preview-image"
            preview-disabled
          />
          <n-button
            class="remove-btn"
            size="tiny"
            circle
            type="error"
            @click="removeImage(image.id)"
          >
            <template #icon>
              <n-icon><Dismiss24Regular /></n-icon>
            </template>
          </n-button>
          <n-tag size="tiny" class="size-tag">
            {{ (image.size / 1024).toFixed(1) }}KB
          </n-tag>
        </div>
      </n-space>
    </div>

    <!-- Upload Button -->
    <div class="upload-section">
      <input
        id="image-input"
        type="file"
        accept="image/*"
        multiple
        style="display: none"
        @change="handleFileSelect"
      />
      <n-button
        size="small"
        @click="handleUploadClick"
        :loading="isProcessing"
        :disabled="selectedImages.length >= 5"
      >
        <template #icon>
          <n-icon><Image24Regular /></n-icon>
        </template>
        {{ selectedImages.length >= 5 ? '最多5张' : '添加图片' }}
      </n-button>
      <span v-if="hasImages" class="image-count">
        {{ selectedImages.length }}/5
      </span>
    </div>
  </div>
</template>

<style scoped>
.image-input-container {
  width: 100%;
}

.images-preview {
  margin-bottom: 8px;
}

.image-preview-item {
  position: relative;
  display: inline-block;
}

.preview-image {
  width: 80px;
  height: 80px;
  object-fit: cover;
  border-radius: 8px;
  border: 1px solid var(--n-border-color);
}

.remove-btn {
  position: absolute;
  top: -8px;
  right: -8px;
  opacity: 0;
  transition: opacity 0.2s;
}

.image-preview-item:hover .remove-btn {
  opacity: 1;
}

.size-tag {
  position: absolute;
  bottom: 4px;
  left: 4px;
  background: rgba(0, 0, 0, 0.6);
  color: white;
}

.upload-section {
  display: flex;
  align-items: center;
  gap: 8px;
}

.image-count {
  font-size: 0.85em;
  color: var(--n-text-color-3);
}
</style>
