<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'
import { emit, listen } from '@tauri-apps/api/event'

interface Message {
  data: string
}

const greetMsg = ref('')
const name = ref('')

async function greet() {
  // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
  greetMsg.value = await invoke('greet', { name: name.value })
}

const message = ref<string>('')
const messageList = ref<Array<string>>([])

const publish = async () => {
  emit('message', { data: message.value })
  message.value = ''
}

listen('receive', (event) => {
  const message = (event.payload as Message).data
  messageList.value.push(message)
  console.log(message)
})
</script>

<template>
  <div class="card">
    <input id="greet-input" v-model="name" placeholder="Enter a name..." />
    <button type="button" @click="greet()">Greet</button>
    <input type="text" v-model="message" @keyup.enter="publish" />
    <button type="button" @click="publish">publish</button>
    <div v-for="message in messageList">收到消息: {{ message }}</div>
  </div>

  <p>{{ greetMsg }}</p>
</template>
