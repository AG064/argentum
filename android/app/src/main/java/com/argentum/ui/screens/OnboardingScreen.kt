package com.argentum.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.clickable
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowForward
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.DarkMode
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.LightMode
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.argentum.ui.components.ArgentumLogo
import com.argentum.ui.components.GlassButton
import com.argentum.ui.components.GlassCard
import com.argentum.ui.components.GlassSurface
import com.argentum.ui.theme.CrimsonRed
import com.argentum.ui.theme.Silver
import com.argentum.viewmodel.OnboardingViewModel

data class Provider(
    val id: String,
    val name: String,
    val description: String,
    val defaultModel: String,
    val icon: String
)

val STABLE_PROVIDERS = listOf(
    Provider("minimax", "MiniMax", "Fast and affordable, great for coding", "MiniMax-M2.7", "⚡"),
    Provider("openai", "OpenAI", "GPT models, stable and reliable", "gpt-4o-mini", "🤖"),
    Provider("local", "Local (llama.cpp)", "Local AI via llama.cpp server", "local-model", "💻")
)

@Composable
fun OnboardingScreen(
    viewModel: OnboardingViewModel,
    onComplete: () -> Unit,
    modifier: Modifier = Modifier
) {
    val uiState by viewModel.uiState.collectAsState()
    var currentStep by remember { mutableIntStateOf(0) }

    val totalSteps = 3

    GlassSurface(modifier = modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
        // Progress indicator
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.Center
        ) {
            repeat(totalSteps) { step ->
                Box(
                    modifier = Modifier
                        .size(if (step == currentStep) 12.dp else 8.dp)
                        .clip(CircleShape)
                        .background(
                            if (step <= currentStep) CrimsonRed
                            else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.3f)
                        )
                )
                if (step < totalSteps - 1) {
                    Spacer(modifier = Modifier.width(8.dp))
                }
            }
        }

        Spacer(modifier = Modifier.height(32.dp))

        // Step content
        AnimatedVisibility(
            visible = currentStep == 0,
            enter = fadeIn(),
            exit = fadeOut()
        ) {
            WelcomeStep(
                isDarkMode = uiState.isDarkMode,
                onToggleDarkMode = { viewModel.toggleDarkMode() }
            )
        }

        AnimatedVisibility(
            visible = currentStep == 1,
            enter = fadeIn(),
            exit = fadeOut()
        ) {
            ProviderStep(
                selectedProvider = uiState.selectedProvider,
                onProviderSelected = { viewModel.selectProvider(it) }
            )
        }

        AnimatedVisibility(
            visible = currentStep == 2,
            enter = fadeIn(),
            exit = fadeOut()
        ) {
            ApiKeyStep(
                provider = uiState.selectedProvider,
                apiKey = uiState.apiKey,
                endpoint = uiState.endpoint,
                model = uiState.model,
                onApiKeyChange = { viewModel.updateApiKey(it) },
                onEndpointChange = { viewModel.updateEndpoint(it) },
                onModelChange = { viewModel.updateModel(it) }
            )
        }

        Spacer(modifier = Modifier.weight(1f))

        // Navigation buttons
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            if (currentStep > 0) {
                TextButton(onClick = { currentStep-- }) {
                    Text("Back", color = Silver)
                }
            } else {
                Spacer(modifier = Modifier.width(1.dp))
            }

            Button(
                onClick = {
                    if (currentStep < totalSteps - 1) {
                        currentStep++
                    } else {
                        viewModel.completeOnboarding()
                        onComplete()
                    }
                },
                colors = ButtonDefaults.buttonColors(
                    containerColor = CrimsonRed
                ),
                shape = RoundedCornerShape(12.dp)
            ) {
                Text(
                    if (currentStep < totalSteps - 1) "Next" else "Get Started"
                )
                if (currentStep < totalSteps - 1) {
                    Spacer(modifier = Modifier.width(8.dp))
                    Icon(
                        imageVector = Icons.Default.ArrowForward,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp)
                    )
                }
            }
        }
    }
    }
}

@Composable
private fun WelcomeStep(
    isDarkMode: Boolean,
    onToggleDarkMode: () -> Unit
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier.fillMaxWidth()
    ) {
        ArgentumLogo(
            modifier = Modifier.padding(top = 8.dp),
            size = 120.dp,
        )

        Spacer(modifier = Modifier.height(16.dp))

        Text(
            text = "Argentum",
            style = MaterialTheme.typography.headlineLarge,
            color = CrimsonRed
        )

        Spacer(modifier = Modifier.height(8.dp))

        Text(
            text = "Your AI Agent Framework",
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
        )

        Spacer(modifier = Modifier.height(40.dp))

        Text(
            text = "Welcome! Let's set up Argentum for you.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center
        )

        Spacer(modifier = Modifier.height(32.dp))

        // Dark mode toggle in a glass card
        GlassCard(
            modifier = Modifier
                .fillMaxWidth()
                .clickable { onToggleDarkMode() },
            cornerRadius = 18.dp,
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    imageVector = if (isDarkMode) Icons.Default.DarkMode else Icons.Default.LightMode,
                    contentDescription = null,
                    tint = CrimsonRed
                )
                Spacer(modifier = Modifier.width(16.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "Dark Mode",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = if (isDarkMode) "Dark theme enabled" else "Light theme enabled",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
                    )
                }
                Icon(
                    imageVector = if (isDarkMode) Icons.Default.Check else Icons.Default.Person,
                    contentDescription = null,
                    tint = if (isDarkMode) CrimsonRed else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f)
                )
            }
        }
    }
}

@Composable
private fun ProviderStep(
    selectedProvider: String,
    onProviderSelected: (String) -> Unit
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier.fillMaxWidth()
    ) {
        Text(
            text = "Choose Your AI Provider",
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface
        )

        Spacer(modifier = Modifier.height(8.dp))

        Text(
            text = "Select a stable provider for chat",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
        )

        Spacer(modifier = Modifier.height(24.dp))

        LazyColumn(
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            items(STABLE_PROVIDERS) { provider ->
                ProviderCard(
                    provider = provider,
                    isSelected = selectedProvider == provider.id,
                    onClick = { onProviderSelected(provider.id) }
                )
            }
        }
    }
}

@Composable
private fun ProviderCard(
    provider: Provider,
    isSelected: Boolean,
    onClick: () -> Unit
) {
    GlassCard(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onClick() },
        cornerRadius = 18.dp,
        backgroundColor = if (isSelected)
            CrimsonRed.copy(alpha = 0.10f)
        else
            MaterialTheme.colorScheme.surface,
        borderColor = if (isSelected)
            CrimsonRed.copy(alpha = 0.55f)
        else
            Silver.copy(alpha = 0.30f),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Box(
                modifier = Modifier
                    .size(48.dp)
                    .clip(CircleShape),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = provider.icon,
                    style = MaterialTheme.typography.headlineSmall
                )
            }

            Spacer(modifier = Modifier.width(16.dp))

            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = provider.name,
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurface
                )
                Text(
                    text = provider.description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
                )
                Text(
                    text = provider.defaultModel,
                    style = MaterialTheme.typography.labelSmall,
                    color = CrimsonRed.copy(alpha = 0.8f)
                )
            }

            if (isSelected) {
                Icon(
                    imageVector = Icons.Default.Check,
                    contentDescription = "Selected",
                    tint = CrimsonRed
                )
            }
        }
    }
}

@Composable
private fun ApiKeyStep(
    provider: String,
    apiKey: String,
    endpoint: String,
    model: String,
    onApiKeyChange: (String) -> Unit,
    onEndpointChange: (String) -> Unit,
    onModelChange: (String) -> Unit
) {
    var passwordVisible by remember { mutableStateOf(false) }

    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier.fillMaxWidth()
    ) {
        Text(
            text = "Configure API",
            style = MaterialTheme.typography.headlineSmall,
            color = MaterialTheme.colorScheme.onSurface
        )

        Spacer(modifier = Modifier.height(8.dp))

        Text(
            text = "Enter your API credentials",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
        )

        Spacer(modifier = Modifier.height(24.dp))

        // API Key
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
            ),
            shape = RoundedCornerShape(12.dp)
        ) {
            Column(modifier = Modifier.padding(16.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        imageVector = Icons.Default.Key,
                        contentDescription = null,
                        tint = CrimsonRed
                    )
                    Spacer(modifier = Modifier.width(8.dp))
                    Text(
                        text = "API Key",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                }

                Spacer(modifier = Modifier.height(12.dp))

                OutlinedTextField(
                    value = apiKey,
                    onValueChange = onApiKeyChange,
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text("Enter your API key") },
                    visualTransformation = if (passwordVisible) VisualTransformation.None else PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = Silver.copy(alpha = 0.5f),
                        unfocusedBorderColor = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f)
                    ),
                    shape = RoundedCornerShape(8.dp),
                    trailingIcon = {
                        TextButton(onClick = { passwordVisible = !passwordVisible }) {
                            Text(
                                if (passwordVisible) "Hide" else "Show",
                                color = CrimsonRed
                            )
                        }
                    }
                )
            }
        }

        Spacer(modifier = Modifier.height(12.dp))

        // Endpoint (for local provider)
        if (provider == "local") {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
                ),
                shape = RoundedCornerShape(12.dp)
            ) {
                Column(modifier = Modifier.padding(16.dp)) {
                    Text(
                        text = "Local Server Endpoint",
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )

                    Spacer(modifier = Modifier.height(12.dp))

                    OutlinedTextField(
                        value = endpoint,
                        onValueChange = onEndpointChange,
                        modifier = Modifier.fillMaxWidth(),
                        placeholder = { Text("http://127.0.0.1:8080/v1") },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = Silver.copy(alpha = 0.5f),
                            unfocusedBorderColor = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f)
                        ),
                        shape = RoundedCornerShape(8.dp),
                        singleLine = true
                    )
                }
            }

            Spacer(modifier = Modifier.height(12.dp))
        }

        // Model
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f)
            ),
            shape = RoundedCornerShape(12.dp)
        ) {
            Column(modifier = Modifier.padding(16.dp)) {
                Text(
                    text = "Model",
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurface
                )

                Spacer(modifier = Modifier.height(12.dp))

                OutlinedTextField(
                    value = model,
                    onValueChange = onModelChange,
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text("Model name") },
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = Silver.copy(alpha = 0.5f),
                        unfocusedBorderColor = MaterialTheme.colorScheme.outline.copy(alpha = 0.3f)
                    ),
                    shape = RoundedCornerShape(8.dp),
                    singleLine = true
                )
            }
        }
    }
}
