package com.argentum

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Chat
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.outlined.Chat
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.viewmodel.compose.viewModel
import com.argentum.ui.screens.ChatScreen
import com.argentum.ui.screens.OnboardingScreen
import com.argentum.ui.screens.SettingsScreen
import com.argentum.ui.theme.ArgentumTheme
import com.argentum.ui.theme.CrimsonRed
import com.argentum.ui.theme.Silver
import com.argentum.viewmodel.ChatViewModel
import com.argentum.viewmodel.ChatViewModelFactory
import com.argentum.viewmodel.OnboardingViewModel
import com.argentum.viewmodel.OnboardingViewModelFactory
import com.argentum.viewmodel.SettingsViewModel
import com.argentum.viewmodel.SettingsViewModelFactory
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            ArgentumApp()
        }
    }
}

sealed class BottomNavItem(
    val title: String,
    val selectedIcon: ImageVector,
    val unselectedIcon: ImageVector
) {
    data object Chat : BottomNavItem(
        title = "Chat",
        selectedIcon = Icons.Filled.Chat,
        unselectedIcon = Icons.Outlined.Chat
    )

    data object Settings : BottomNavItem(
        title = "Settings",
        selectedIcon = Icons.Filled.Settings,
        unselectedIcon = Icons.Outlined.Settings
    )
}

@Composable
fun ArgentumApp() {
    val context = LocalContext.current
    val settingsViewModel: SettingsViewModel = viewModel(factory = SettingsViewModelFactory(context))
    val settingsState by settingsViewModel.uiState.collectAsState()

    // Check onboarding status
    val onboardingViewModel: OnboardingViewModel = viewModel(factory = OnboardingViewModelFactory(context))
    val onboardingState by onboardingViewModel.uiState.collectAsState()

    // RunBlocking to check onboarding synchronously on first composition
    var isOnboardingComplete by remember {
        var value = false
        runBlocking {
            value = settingsViewModel.isOnboardingComplete()
        }
        mutableIntStateOf(if (value) 1 else 0)
    }

    ArgentumTheme(darkTheme = settingsState.isDarkMode) {
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = MaterialTheme.colorScheme.background
        ) {
            if (isOnboardingComplete.intValue == 0 && !onboardingState.isComplete) {
                // Show onboarding
                OnboardingScreen(
                    viewModel = onboardingViewModel,
                    onComplete = {
                        isOnboardingComplete.intValue = 1
                    }
                )
            } else {
                // Show main app
                MainContent(
                    settingsViewModel = settingsViewModel,
                    chatViewModel = viewModel(factory = ChatViewModelFactory(context))
                )
            }
        }
    }
}

@Composable
private fun MainContent(
    settingsViewModel: SettingsViewModel,
    chatViewModel: ChatViewModel
) {
    var selectedTab by remember { mutableIntStateOf(0) }

    val navItems = listOf(
        BottomNavItem.Chat,
        BottomNavItem.Settings
    )

    Scaffold(
        bottomBar = {
            ArgentumBottomNavigation(
                items = navItems,
                selectedIndex = selectedTab,
                onItemSelected = { selectedTab = it }
            )
        }
    ) { paddingValues ->
        Surface(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues),
            color = MaterialTheme.colorScheme.background
        ) {
            when (selectedTab) {
                0 -> ChatScreen(viewModel = chatViewModel)
                1 -> SettingsScreen(viewModel = settingsViewModel)
            }
        }
    }
}

@Composable
private fun ArgentumBottomNavigation(
    items: List<BottomNavItem>,
    selectedIndex: Int,
    onItemSelected: (Int) -> Unit,
    modifier: Modifier = Modifier
) {
    NavigationBar(
        modifier = modifier,
        containerColor = MaterialTheme.colorScheme.surface.copy(alpha = 0.95f),
        contentColor = MaterialTheme.colorScheme.onSurface
    ) {
        items.forEachIndexed { index, item ->
            val selected = selectedIndex == index

            val iconColor by animateColorAsState(
                targetValue = if (selected)
                    CrimsonRed
                else
                    MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                animationSpec = tween(durationMillis = 200),
                label = "navIconColor"
            )

            NavigationBarItem(
                selected = selected,
                onClick = { onItemSelected(index) },
                icon = {
                    Icon(
                        imageVector = if (selected) item.selectedIcon else item.unselectedIcon,
                        contentDescription = item.title,
                        tint = iconColor
                    )
                },
                label = {
                    Text(
                        text = item.title,
                        style = MaterialTheme.typography.labelMedium,
                        color = iconColor
                    )
                },
                colors = NavigationBarItemDefaults.colors(
                    selectedIconColor = CrimsonRed,
                    selectedTextColor = CrimsonRed,
                    unselectedIconColor = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                    unselectedTextColor = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                    indicatorColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.8f)
                )
            )
        }
    }
}
